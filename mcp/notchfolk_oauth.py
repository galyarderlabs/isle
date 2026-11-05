# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Galyarder Labs
"""OAuth authorization for MCP clients that cannot send static bearer keys."""

from __future__ import annotations

import hashlib
import hmac
import html
import json
import os
import secrets
import sqlite3
import time
from contextlib import contextmanager
from pathlib import Path
from urllib.parse import parse_qs

from fastmcp.server.auth import OAuthProvider
from fastmcp.server.auth.auth import ClientRegistrationOptions, RevocationOptions
from mcp.server.auth.provider import (
    AccessToken,
    AuthorizationCode,
    AuthorizationParams,
    AuthorizeError,
    RefreshToken,
    TokenError,
    construct_redirect_uri,
)
from mcp.shared.auth import OAuthClientInformationFull, OAuthToken
from starlette.middleware import Middleware
from starlette.requests import Request
from starlette.responses import HTMLResponse, JSONResponse, RedirectResponse, Response
from starlette.routing import Route

ACCESS_TTL = 60 * 60
REFRESH_TTL = 60 * 60 * 24 * 30
CODE_TTL = 5 * 60
LOGIN_WINDOW = 15 * 60
LOGIN_LIMIT = 5
MAX_FORM_BYTES = 8192


class OAuthEndpointRateLimit:
    """Small per-process limiter for this single-user, single-process server."""

    LIMITS = {
        "/authorize": (60, 30),
        "/register": (60, 10),
        "/token": (60, 60),
        "/revoke": (60, 30),
    }

    def __init__(self, app):
        self.app = app
        self.buckets: dict[tuple[str, str], list[float]] = {}

    async def __call__(self, scope, receive, send):
        limit = self.LIMITS.get(scope.get("path", "")) if scope.get("type") == "http" else None
        if limit:
            window, maximum = limit
            client = scope.get("client")
            ip = client[0] if client else "unknown"
            key = (ip, scope["path"])
            now = time.monotonic()
            recent = [stamp for stamp in self.buckets.get(key, []) if now - stamp < window]
            if len(recent) >= maximum:
                response = JSONResponse(
                    {"error": "temporarily_unavailable"}, status_code=429,
                    headers={"Retry-After": str(window), "Cache-Control": "no-store"},
                )
                await response(scope, receive, send)
                return
            recent.append(now)
            self.buckets[key] = recent
            if len(self.buckets) > 4096:
                self.buckets = {k: v for k, v in self.buckets.items()
                                if v and now - v[-1] < window}
        await self.app(scope, receive, send)


def _digest(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


class NotchfolkOAuthProvider(OAuthProvider):
    """Issues OAuth tokens after validating an existing Notchfolk MCP bearer key."""

    def __init__(self, *, base_url: str, legacy_tokens: dict[str, dict], scopes: list[str], db_path: str):
        super().__init__(
            base_url=base_url.rstrip("/"),
            resource_base_url=base_url.rstrip("/"),
            client_registration_options=ClientRegistrationOptions(
                enabled=True, valid_scopes=scopes, default_scopes=scopes
            ),
            revocation_options=RevocationOptions(enabled=True),
        )
        self.legacy_tokens = dict(legacy_tokens)
        self._login_attempts: dict[str, list[float]] = {}
        self.db_path = Path(db_path).expanduser()
        self.db_path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with self._db() as db:
            db.executescript(
                """
                CREATE TABLE IF NOT EXISTS clients (id TEXT PRIMARY KEY, data TEXT NOT NULL);
                CREATE TABLE IF NOT EXISTS transactions (
                    id_hash TEXT PRIMARY KEY, data TEXT NOT NULL, expires REAL NOT NULL
                );
                CREATE TABLE IF NOT EXISTS codes (
                    code_hash TEXT PRIMARY KEY, data TEXT NOT NULL, expires REAL NOT NULL
                );
                CREATE TABLE IF NOT EXISTS tokens (
                    token_hash TEXT PRIMARY KEY, kind TEXT NOT NULL, family TEXT NOT NULL,
                    data TEXT NOT NULL, expires REAL NOT NULL
                );
                CREATE INDEX IF NOT EXISTS tokens_family ON tokens(family);
                """
            )
            now = time.time()
            db.execute("DELETE FROM transactions WHERE expires < ?", (now,))
            db.execute("DELETE FROM codes WHERE expires < ?", (now,))
            db.execute("DELETE FROM tokens WHERE expires < ?", (now,))
        if self.db_path.exists():
            os.chmod(self.db_path, 0o600)

    @contextmanager
    def _db(self):
        db = sqlite3.connect(self.db_path, timeout=5)
        db.execute("PRAGMA busy_timeout=5000")
        try:
            yield db
            db.commit()
        except Exception:
            db.rollback()
            raise
        finally:
            db.close()

    def get_routes(self, mcp_path: str | None = None) -> list[Route]:
        return [
            *super().get_routes(mcp_path),
            Route("/oauth/authorize", self._authorization_page, methods=["GET", "POST"]),
        ]

    def get_middleware(self) -> list[Middleware]:
        return [*super().get_middleware(), Middleware(OAuthEndpointRateLimit)]

    async def get_client(self, client_id: str) -> OAuthClientInformationFull | None:
        with self._db() as db:
            row = db.execute("SELECT data FROM clients WHERE id = ?", (client_id,)).fetchone()
        return OAuthClientInformationFull.model_validate_json(row[0]) if row else None

    async def register_client(self, client_info: OAuthClientInformationFull) -> None:
        with self._db() as db:
            db.execute(
                "INSERT OR REPLACE INTO clients(id, data) VALUES (?, ?)",
                (client_info.client_id, client_info.model_dump_json()),
            )

    async def authorize(self, client: OAuthClientInformationFull, params: AuthorizationParams) -> str:
        allowed = set(self.client_registration_options.valid_scopes or [])
        if any(scope not in allowed for scope in (params.scopes or [])):
            raise AuthorizeError(error="invalid_scope", error_description="Unsupported Notchfolk scope")
        if params.resource and self._resource_url and params.resource != str(self._resource_url):
            raise AuthorizeError(error="invalid_target", error_description="This key is for a different MCP server")
        tx = secrets.token_urlsafe(32)
        data = {
            "client_id": client.client_id,
            "redirect_uri": str(params.redirect_uri),
            "redirect_uri_provided_explicitly": params.redirect_uri_provided_explicitly,
            "state": params.state,
            "scopes": params.scopes or [],
            "code_challenge": params.code_challenge,
            "resource": params.resource or (str(self._resource_url) if self._resource_url else None),
        }
        with self._db() as db:
            db.execute(
                "INSERT INTO transactions(id_hash, data, expires) VALUES (?, ?, ?)",
                (_digest(tx), json.dumps(data), time.time() + CODE_TTL),
            )
        return f"{str(self.base_url).rstrip('/')}/oauth/authorize?tx={tx}"

    def _transaction(self, tx: str) -> dict | None:
        with self._db() as db:
            row = db.execute(
                "SELECT data, expires FROM transactions WHERE id_hash = ?", (_digest(tx),)
            ).fetchone()
            if row and row[1] < time.time():
                db.execute("DELETE FROM transactions WHERE id_hash = ?", (_digest(tx),))
                return None
        return json.loads(row[0]) if row else None

    def _client_ip(self, request: Request) -> str:
        return request.client.host if request.client else "unknown"

    def _limited(self, ip: str) -> bool:
        now = time.time()
        attempts = [t for t in self._login_attempts.get(ip, []) if now - t < LOGIN_WINDOW]
        self._login_attempts[ip] = attempts
        return len(attempts) >= LOGIN_LIMIT

    def _failed_attempt(self, ip: str) -> None:
        self._login_attempts.setdefault(ip, []).append(time.time())

    async def _authorization_page(self, request: Request) -> Response:
        if request.method == "GET":
            tx = request.query_params.get("tx", "")
            data = self._transaction(tx) if len(tx) <= 128 else None
            if not data:
                return self._page("This authorization link expired. Start the connection again.", status=400)
            client = await self.get_client(data["client_id"])
            name = html.escape(client.client_name or "MCP client") if client else "MCP client"
            body = f"""
                <p><strong>{name}</strong> wants to connect to Notchfolk.</p>
                <p>Enter the Notchfolk MCP key you already created. Access will use that key's scopes.</p>
                <form method="post" action="/oauth/authorize" autocomplete="off">
                  <input type="hidden" name="tx" value="{html.escape(tx, quote=True)}">
                  <label for="key">Notchfolk MCP key</label>
                  <input id="key" name="key" type="password" required autocomplete="off"
                         spellcheck="false" autofocus>
                  <button type="submit">Connect</button>
                </form>
                <small>Your key is checked by Notchfolk and is never sent back to the MCP client.</small>
            """
            return self._page(body)

        content_type = request.headers.get("content-type", "").split(";", 1)[0].strip().lower()
        if content_type != "application/x-www-form-urlencoded":
            return self._page("Invalid form submission.", status=400)
        try:
            raw = bytearray()
            async for chunk in request.stream():
                raw.extend(chunk)
                if len(raw) > MAX_FORM_BYTES:
                    return self._page("Form is too large.", status=413)
            form = parse_qs(raw.decode("utf-8", "strict"), keep_blank_values=True, max_num_fields=4)
            tx = form.get("tx", [""])[0]
            presented = form.get("key", [""])[0]
        except (UnicodeDecodeError, ValueError):
            return self._page("Invalid form submission.", status=400)

        data = self._transaction(tx) if len(tx) <= 128 else None
        if not data:
            return self._page("This authorization link expired. Start the connection again.", status=400)
        ip = self._client_ip(request)
        if self._limited(ip):
            return self._page("Too many attempts. Wait 15 minutes, then try again.", status=429,
                              headers={"Retry-After": str(LOGIN_WINDOW)})

        entry = None
        for known_key, known_entry in self.legacy_tokens.items():
            if hmac.compare_digest(known_key, presented):
                entry = known_entry
        if not entry:
            self._failed_attempt(ip)
            return self._retry_page(tx, "That key could not be verified. Check it and try again.")

        key_scopes = list(dict.fromkeys(entry.get("scopes", [])))
        requested = data.get("scopes", [])
        if requested:
            granted = [scope for scope in requested if scope in key_scopes or
                       (scope.endswith(":read") and "*:read" in key_scopes)]
            if len(granted) != len(requested):
                return self._retry_page(tx, "This key does not include all permissions requested by the client.")
        else:
            granted = key_scopes
        if not granted:
            return self._retry_page(tx, "This key has no MCP permissions. Create a key with read scopes in Notchfolk Settings.")

        code = secrets.token_urlsafe(32)
        auth_code = AuthorizationCode(
            code=code,
            client_id=data["client_id"],
            redirect_uri=data["redirect_uri"],
            redirect_uri_provided_explicitly=data["redirect_uri_provided_explicitly"],
            scopes=granted,
            expires_at=time.time() + CODE_TTL,
            code_challenge=data["code_challenge"],
            resource=data.get("resource"),
            subject=entry.get("sub"),
        )
        key_hash = _digest(next(secret for secret, meta in self.legacy_tokens.items() if meta is entry))
        with self._db() as db:
            db.execute("DELETE FROM transactions WHERE id_hash = ?", (_digest(tx),))
            code_data = json.loads(auth_code.model_dump_json(exclude={"code"}))
            code_data["notchfolk_key_hash"] = key_hash
            db.execute(
                "INSERT INTO codes(code_hash, data, expires) VALUES (?, ?, ?)",
                (_digest(code), json.dumps(code_data, separators=(",", ":")), auth_code.expires_at),
            )
        destination = construct_redirect_uri(
            str(data["redirect_uri"]), code=code, state=data.get("state")
        )
        return RedirectResponse(destination, status_code=303, headers={"Cache-Control": "no-store", "Referrer-Policy": "no-referrer"})

    def _retry_page(self, tx: str, error: str) -> Response:
        escaped = html.escape(error)
        body = f"""
            <p class="error">{escaped}</p>
            <form method="post" action="/oauth/authorize" autocomplete="off">
              <input type="hidden" name="tx" value="{html.escape(tx, quote=True)}">
              <label for="key">Notchfolk MCP key</label>
              <input id="key" name="key" type="password" required autocomplete="off" spellcheck="false" autofocus>
              <button type="submit">Try again</button>
            </form>
        """
        return self._page(body, status=401)

    def _page(self, body: str, *, status: int = 200, headers: dict | None = None) -> HTMLResponse:
        page = f"""<!doctype html><html lang="en"><meta charset="utf-8">
          <meta name="viewport" content="width=device-width,initial-scale=1">
          <title>Connect Notchfolk</title>
          <style>
            *{{box-sizing:border-box}} body{{margin:0;min-height:100vh;display:grid;place-items:center;
            padding:24px;background:#080b0a;color:#eef8f0;font:15px system-ui,sans-serif}}
            main{{width:min(440px,100%);padding:28px;border:1px solid #264b37;border-radius:18px;
            background:linear-gradient(145deg,#111915,#090d0b);box-shadow:0 0 42px #13b65a20}}
            h1{{margin:0 0 18px;font-size:24px}} p{{line-height:1.55;color:#c3d1c8}} small{{display:block;
            margin-top:16px;color:#8ea296;line-height:1.5}} label{{display:block;margin:20px 0 7px}}
            input{{width:100%;padding:12px;border-radius:10px;border:1px solid #385644;background:#080d0a;
            color:#fff;font:inherit}} button{{width:100%;margin-top:14px;padding:12px;border:0;border-radius:10px;
            background:#36d477;color:#041309;font:600 15px system-ui;cursor:pointer}} .error{{color:#ff9b9b}}
          </style><main><h1>Connect Notchfolk</h1>{body}</main></html>"""
        response_headers = {
            "Cache-Control": "no-store",
            "Pragma": "no-cache",
            "Referrer-Policy": "no-referrer",
            "X-Content-Type-Options": "nosniff",
            "X-Frame-Options": "DENY",
            "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
        }
        response_headers.update(headers or {})
        return HTMLResponse(page, status_code=status, headers=response_headers)

    async def load_authorization_code(self, client: OAuthClientInformationFull, authorization_code: str) -> AuthorizationCode | None:
        with self._db() as db:
            row = db.execute("SELECT data, expires FROM codes WHERE code_hash = ?", (_digest(authorization_code),)).fetchone()
            if not row:
                return None
            if row[1] < time.time():
                db.execute("DELETE FROM codes WHERE code_hash = ?", (_digest(authorization_code),))
                return None
        data = json.loads(row[0])
        data["code"] = authorization_code
        code = AuthorizationCode.model_validate(data)
        return code if code.client_id == client.client_id else None

    def _store_token(self, db: sqlite3.Connection, kind: str, token: str, family: str, data: dict, expiry: int) -> None:
        db.execute(
            "INSERT INTO tokens(token_hash, kind, family, data, expires) VALUES (?, ?, ?, ?, ?)",
            (_digest(token), kind, family, json.dumps(data, separators=(",", ":")), expiry),
        )

    def _issue_pair(self, db: sqlite3.Connection, client_id: str, scopes: list[str], resource: str | None,
                    subject: str | None, claims: dict, family: str | None = None) -> OAuthToken:
        family = family or secrets.token_urlsafe(24)
        access_value = secrets.token_urlsafe(32)
        refresh_value = secrets.token_urlsafe(32)
        now = int(time.time())
        access = AccessToken(token=access_value, client_id=client_id, scopes=scopes,
                             expires_at=now + ACCESS_TTL, resource=resource, subject=subject, claims=claims)
        refresh = RefreshToken(token=refresh_value, client_id=client_id, scopes=scopes,
                               expires_at=now + REFRESH_TTL, resource=resource, subject=subject)
        self._store_token(db, "access", access_value, family,
                          access.model_dump(mode="json", exclude={"token"}), access.expires_at)
        refresh_data = refresh.model_dump(mode="json", exclude={"token"})
        refresh_data["notchfolk_key_hash"] = claims.get("notchfolk_key_hash")
        self._store_token(db, "refresh", refresh_value, family, refresh_data, refresh.expires_at)
        return OAuthToken(access_token=access_value, token_type="Bearer", expires_in=ACCESS_TTL,
                          refresh_token=refresh_value, scope=" ".join(scopes))

    async def exchange_authorization_code(self, client: OAuthClientInformationFull, authorization_code: AuthorizationCode) -> OAuthToken:
        with self._db() as db:
            db.execute("BEGIN IMMEDIATE")
            row = db.execute("SELECT data, expires FROM codes WHERE code_hash = ?", (_digest(authorization_code.code),)).fetchone()
            if not row or row[1] < time.time():
                raise TokenError("invalid_grant", "Authorization code expired or already used")
            data = json.loads(row[0])
            key_hash = data.pop("notchfolk_key_hash", None)
            data["code"] = authorization_code.code
            stored = AuthorizationCode.model_validate(data)
            if stored.client_id != client.client_id:
                raise TokenError("invalid_grant", "Authorization code belongs to another client")
            db.execute("DELETE FROM codes WHERE code_hash = ?", (_digest(stored.code),))
            key_name = stored.subject or ""
            legacy = next(((token, meta) for token, meta in self.legacy_tokens.items()
                           if meta.get("sub") == key_name and _digest(token) == key_hash), None)
            # A key may have been revoked after the consent page was opened.
            if not legacy:
                raise TokenError("invalid_grant", "The Notchfolk key has been revoked")
            claims = {"notchfolk_client": key_name, "notchfolk_key_hash": _digest(legacy[0])}
            return self._issue_pair(db, client.client_id, stored.scopes, stored.resource,
                                    key_name, claims)

    def _load_token(self, token: str, kind: str):
        with self._db() as db:
            row = db.execute("SELECT data, expires FROM tokens WHERE token_hash = ? AND kind = ?",
                             (_digest(token), kind)).fetchone()
            if not row:
                return None
            if row[1] < time.time():
                db.execute("DELETE FROM tokens WHERE token_hash = ?", (_digest(token),))
                return None
        model = AccessToken if kind == "access" else RefreshToken
        return model.model_validate({**json.loads(row[0]), "token": token})

    async def load_access_token(self, token: str) -> AccessToken | None:
        return self._load_token(token, "access")

    async def verify_token(self, token: str) -> AccessToken | None:
        oauth_token = await self.load_access_token(token)
        if oauth_token:
            claims = oauth_token.claims or {}
            key_name = claims.get("notchfolk_client")
            key_hash = claims.get("notchfolk_key_hash")
            current = next(((secret, meta) for secret, meta in self.legacy_tokens.items()
                            if meta.get("sub") == key_name and _digest(secret) == key_hash), None)
            if not current:
                return None
            current_scopes = set(current[1].get("scopes", []))
            granted = []
            for scope in oauth_token.scopes:
                if scope in current_scopes or (scope.endswith(":read") and "*:read" in current_scopes):
                    granted.append(scope)
                elif scope == "*:read":
                    granted.extend(s for s in current_scopes if s.endswith(":read"))
            return oauth_token.model_copy(update={"scopes": list(dict.fromkeys(granted))})
        for known_key, metadata in self.legacy_tokens.items():
            if hmac.compare_digest(known_key, token):
                return AccessToken(token=token, client_id=metadata["client_id"],
                                   scopes=metadata.get("scopes", []), subject=metadata.get("sub"))
        return None

    async def load_refresh_token(self, client: OAuthClientInformationFull, refresh_token: str) -> RefreshToken | None:
        token = self._load_token(refresh_token, "refresh")
        return token if token and token.client_id == client.client_id else None

    async def exchange_refresh_token(self, client: OAuthClientInformationFull, refresh_token: RefreshToken,
                                     scopes: list[str]) -> OAuthToken:
        if not set(scopes).issubset(refresh_token.scopes):
            raise TokenError("invalid_scope", "Requested scopes exceed the original Notchfolk key")
        with self._db() as db:
            db.execute("BEGIN IMMEDIATE")
            row = db.execute("SELECT family, data FROM tokens WHERE token_hash = ? AND kind = 'refresh'",
                             (_digest(refresh_token.token),)).fetchone()
            if not row:
                raise TokenError("invalid_grant", "Refresh token expired or already used")
            family = row[0]
            old_claims = json.loads(row[1])
            current = await self.verify_token_from_claims(
                refresh_token.subject, old_claims.get("notchfolk_key_hash")
            )
            if not current:
                raise TokenError("invalid_grant", "The Notchfolk key has been revoked")
            current_scopes = current.pop("scopes")
            safe_scopes = []
            for scope in scopes:
                if scope in current_scopes or (scope.endswith(":read") and "*:read" in current_scopes):
                    safe_scopes.append(scope)
                elif scope == "*:read":
                    safe_scopes.extend(s for s in current_scopes if s.endswith(":read"))
                else:
                    raise TokenError("invalid_scope", "The Notchfolk key no longer grants this scope")
            db.execute("DELETE FROM tokens WHERE family = ?", (family,))
            return self._issue_pair(db, client.client_id, list(dict.fromkeys(safe_scopes)),
                                    refresh_token.resource, refresh_token.subject, current, family)

    async def verify_token_from_claims(self, key_name: str | None, expected_hash: str | None) -> dict | None:
        for token, metadata in self.legacy_tokens.items():
            if metadata.get("sub") == key_name and hmac.compare_digest(_digest(token), expected_hash or ""):
                return {"notchfolk_client": key_name, "notchfolk_key_hash": _digest(token),
                        "scopes": metadata.get("scopes", [])}
        return None

    async def revoke_token(self, token: AccessToken | RefreshToken) -> None:
        with self._db() as db:
            row = db.execute("SELECT family FROM tokens WHERE token_hash = ?", (_digest(token.token),)).fetchone()
            if row:
                db.execute("DELETE FROM tokens WHERE family = ?", (row[0],))

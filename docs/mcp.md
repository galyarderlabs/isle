# Isle MCP server for external AI agents

Isle exposes local Isle data and actions as MCP tools for AI agents running
outside Isle. The agent's MCP settings (for example, in Claude Code,
Antigravity, or Codex) point to this service; this is not a provider or internal tool interface for
the assistant inside Isle. This page documents the external service that is
currently implemented.

## Current implementation boundary

This is the optional **external** MCP service. It is not the tool interface
used by the assistant running inside Isle. The in-Isle assistant uses its
existing answer/action loop with Hermes or OMP; it does not need to call this
MCP server. Configure the MCP connection in the external agent's own settings,
not in Isle's Assistant Settings.

The current service exposes tasks, Isle calendar events, reminders, calorie
entries, water entries, prayer schedule/location, current media controls,
focus timer state and controls, and HTTP(S) URL opening. It does not yet cover
every Isle page or action. For example, the clipboard or shelf contents,
notifications, calls, and other desktop actions are not in this service's tool
list.

The target is an external MCP tool surface that covers all Isle data and
actions. It must support useful reads as well as writes: for example, an
external agent should be able to request calories for a specific date or date
range and receive those records, such as
`list_food_entries(date="2026-02-27")`. Calendar, calorie, and water reads
now take `date` and `start_date`/`end_date` filters.
Inventory every Isle page and capability. Any exclusion needs a specific
technical or security reason. Calls should use Isle services and enforce
client scopes, write allowlists, and auditing. The target and its incomplete work are tracked in
[assistant-platform-plan.md](assistant-platform-plan.md)
and [assistant-platform-tasks.md](assistant-platform-tasks.md).

Install the optional Python runtime first:

```sh
isle-setup-mcp
```

The service listens on loopback port `8765` by default. To enable write tools,
create `~/.config/isle/mcp.env` with mode `0600` and list only the tools you
intend to allow:

```ini
ISLE_MCP_HOST=127.0.0.1
ISLE_MCP_PORT=8765
ISLE_MCP_WRITE_TOOLS=add_task,add_reminder,add_food_entry
```

Enable the user service:

```sh
systemctl --user enable --now isle-mcp.service
```

The endpoint is `http://127.0.0.1:8765/mcp`. Add this endpoint, and the bearer
token if configured, to the MCP settings of the external agent that should use
Isle. Each agent has its own way to store MCP server connections. For a
network bind, set a bearer
token in the private environment file too:

```ini
ISLE_MCP_HOST=0.0.0.0
ISLE_MCP_TOKEN=replace-with-a-long-random-token
```

The bearer token protects calls, but HTTP does not encrypt them. Keep remote
access on a trusted network or put the service behind a TLS reverse proxy; do
not expose it directly to an untrusted network. Configure each MCP client with
the bearer token when the service is bound beyond loopback.

For access from another machine over the internet, keep the server on loopback
and run a Cloudflare tunnel. `isle-mcp-tunnel` wraps `cloudflared`; TLS is
terminated by Cloudflare and the per-client tokens still apply. Two ways:

### Quick tunnel (temporary URL, no Cloudflare account)

1. Install [`cloudflared`](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/downloads/).
2. Generate at least one key (Settings → MCP → Generate key, or `isle-mcp-keygen add <name> [scopes]`);
   the tunnel refuses to start without one.
3. Run `isle-mcp-tunnel run` and copy the `https://<random>.trycloudflare.com` URL it prints.
4. In the agent's MCP settings, point it at `https://<random>.trycloudflare.com/mcp` with the key as bearer token.

The URL is temporary and changes on every run — use the named tunnel for
anything lasting.

### Named tunnel (your own domain, automatic DNS)

Settings → MCP → Remote access does this from the UI: enter your hostname
(e.g. `isle.example.com`, a zone in your Cloudflare account) and press
**Set up**. The same steps on the command line, transparently:

1. `isle-mcp-tunnel setup isle.example.com` — the script:
   1. Logs in to Cloudflare (`cloudflared tunnel login`; the browser opens once,
      the certificate lands in `~/.cloudflared/cert.pem`).
   2. Creates a tunnel named `isle-mcp` (reusing it if it exists).
   3. Points the hostname at the tunnel: `cloudflared tunnel route dns` — the
      DNS CNAME record is created automatically, nothing to edit by hand.
   4. Writes `~/.config/isle/cloudflared/config.yml` (hostname →
      `http://127.0.0.1:8765`).
   5. Installs and starts the `isle-mcp-tunnel.service` systemd user service.
2. Point the agent at `https://isle.example.com/mcp` with a client key as
   bearer token.

Check and stop:

```sh
isle-mcp-tunnel status
isle-mcp-tunnel stop
```

`stop` disables the service; delete the DNS record in Cloudflare's dashboard to
remove the hostname for good.

For separate permissions per client, create `~/.config/isle/mcp-clients.json`
with mode `0600` and remove `ISLE_MCP_TOKEN` from `mcp.env`:

```json
{
  "calendar-reader": {"token": "a-long-random-token", "scopes": ["calendar:read", "prayer:read"]},
  "life-helper": {"token": "a-different-long-random-token", "scopes": ["*:read", "tasks:write", "reminders:write"]}
}
```

Instead of writing that file by hand, manage keys from Isle Settings → MCP
(generate a key with its scopes, list clients, revoke one; the file is kept at
mode `0600`), or with the command-line helper:

```sh
isle-mcp-keygen add calendar-reader calendar:read prayer:read
isle-mcp-keygen list
isle-mcp-keygen revoke calendar-reader
```

`isle-mcp-keygen add` prints the token once; restart `isle-mcp.service` after
adding or revoking a client.

Read scopes are `tasks:read`, `calendar:read`, `reminders:read`,
`calories:read`, `water:read`, `prayer:read`, `media:read`, `focus:read`,
`apps:read`, `system:read`, `shelf:read`, `clipboard:read`, `router:read`,
and `usage:read`. The `*:read`
scope grants all read-only tools. Write scopes use the same category followed
by `:write`; they still require the global `ISLE_MCP_WRITE_TOOLS` allowlist.
Each token is bound to its own scopes and client name. A caller's MCP
`client_id` is not used as an authorization identity.

Read tools expose Isle tasks, calendar events, reminders, calorie entries,
water entries, prayer times, focus timer state, current media, app usage,
system state, AI router stats, AI plan usage, the shelf, and clipboard history
by search. Calendar
events and calorie and water entries can be filtered by `date` or a
`start_date`/`end_date` range (`YYYY-MM-DD`), and prayer times can be requested
for a `date`. Clipboard reads are search-first (`search_clipboard` returns
ids, `get_clipboard_text` reads one item), so full history is never returned in
one call. Write tools are disabled
unless named in `ISLE_MCP_WRITE_TOOLS`. Available write names are `add_task`,
`set_task_done`, `rename_task`, `remove_task`, `clear_done_tasks`,
`add_calendar_event`, `add_calendar_event_to`, `remove_calendar_event`,
`add_reminder`, `dismiss_reminder`, `add_food_entry`,
`update_food_entry`, `remove_food_entry`, `log_water`, `remove_water_entry`,
`set_prayer_location`, `start_focus`,
`pause_focus`, `stop_focus`, `skip_focus`, `start_focus_phase`, `control_media`,
`media_seek`, `raise_media`, `choose_media_player`, `add_shelf_files`,
`remove_shelf_item`, `clear_shelf`, `refresh_shelf`, `pin_clipboard`,
`remove_clipboard_item`,
and `open_url`. `control_media`
permits play/pause, next, and previous. `open_url` opens an HTTP(S) URL through
the desktop portal and needs the `desktop:write` scope and the matching write
allowlist entry. Read tools also cover the desktop calendars a new event can
go to and their meeting links, the running media players and lyrics, the
focus configuration, and the newer Isle version when one is out.

Prayer city and country are set in Isle Settings → Island → Daily Life; the local
calorie log is stored at `~/.local/share/isle/calories.json`. Food logging is
manual and does not estimate calories from meal names.

This current MCP surface uses Isle's public D-Bus interfaces. Clipboard reads
are search-first and never return the whole history at once; assistant and
agent-permission interfaces stay internal and are not exposed. The shell-only
assistant control API is an internal control
interface, not an Isle user feature exposed by this MCP service.

## Live acceptance checklist

Run these on a machine with isled, the Isle extension, and a session bus;
record exact versions and results in
[assistant-platform-status.md](assistant-platform-status.md) when they pass.

1. **Service**: `isle-setup-mcp`, then `systemctl --user enable --now
   isle-mcp.service`; `systemctl --user is-active isle-mcp.service` says
   `active`, and `curl -s http://127.0.0.1:8765/mcp` answers (not connection
   refused).
2. **Tools/list**: with an external MCP client (Claude Code: `claude mcp add
   --transport http isle http://127.0.0.1:8765/mcp`), `tools/list` returns all
   54 tools with their scopes in the descriptions.
3. **Reads**: `list_tasks`, `list_calendar_events`, `list_reminders`,
   `get_water_entries`, `prayer_schedule`, `now_playing`, `focus_state`,
   `get_app_usage`, `get_system_state`, `get_router_stats`, `get_usage_quota`,
   `list_shelf_items`, `get_latest_update` return real data.
4. **Date filters**: `list_food_entries(date="<today>")` and
   `list_calendar_events(start_date=…, end_date=…)` return only matching
   records; an invalid date is rejected with a clear error.
5. **Writes**: with `ISLE_MCP_WRITE_TOOLS=add_task,log_water` set in `mcp.env`,
   `add_task` and `log_water` succeed and show up in the Isle UI; a write not
   in the allowlist (e.g. `add_reminder`) is refused; writes work again after
   `systemctl --user restart isle-mcp.service`.
6. **Auth**: generate a key with Settings → MCP (or `isle-mcp-keygen`); without
   the bearer token the client gets 401; with `tasks:read` only, `list_tasks`
   works and `add_task` is refused; revoking the key blocks the client after a
   restart.
7. **Remote**: `isle-mcp-tunnel run` prints a `trycloudflare.com` URL and the
   agent reaches `…/mcp` with its token; `isle-mcp-tunnel setup <hostname>`
   logs in, creates the DNS record (visible in Cloudflare's dashboard), and the
   agent reaches `https://<hostname>/mcp`; `isle-mcp-tunnel stop` cuts access.

# Notchfolk Assistant Runtimes and External MCP

Notchfolk's in-app assistant target is Hermes Agent and Oh My Pi (OMP). A separate
MCP server exposes Notchfolk data and actions to AI agents running outside Notchfolk.
The external agent connects using its own MCP settings; it is not another
provider in Notchfolk's Assistant Settings. See the [platform plan](assistant-platform-plan.md)
and [implementation audit](assistant-platform-status.md).

## Hermes Agent inside Notchfolk

Notchfolk connects to Hermes' local API server; it does not start the `hermes`
program itself. Hermes must be installed, configured with a provider/model,
and running its gateway before Notchfolk can send it a request.

### Set up the Hermes API

1. Install Hermes Agent and finish its provider setup using the
   [Hermes installation guide](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/getting-started/installation.md).
2. Add these values to `~/.hermes/.env`:

   ```dotenv
   API_SERVER_ENABLED=true
   API_SERVER_KEY=<paste-a-long-random-secret-here>
   ```

   Generate the key with `openssl rand -hex 32`; paste its output as the value
   of `API_SERVER_KEY`. Keep the file private by setting its mode to `0600`
   (`chmod 600 ~/.hermes/.env`). Keep Hermes on its default API address,
   `127.0.0.1:8642`. The API server exposes Hermes' full toolset, including
   terminal commands, and requires the key even on loopback. Do not bind it to
   `0.0.0.0` for Notchfolk. See Hermes' [API server guide](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/user-guide/features/api-server.md)
   for its security and configuration details.
3. Start the gateway. To run it in the foreground:

   ```sh
   hermes gateway
   ```

   Its startup output should report the API server listening at
   `http://127.0.0.1:8642`. For a persistent user service, install it once
   with `hermes gateway install`, then use `hermes gateway start` and check
   it with `hermes gateway status` (see the Hermes
   [gateway command reference](https://github.com/NousResearch/hermes-agent/blob/main/website/docs/reference/cli-commands.md)).
   Notchfolk does not start or supervise Hermes.
4. Open **Notchfolk Settings → Assistant**, enable the assistant, and choose
   **Hermes Agent**. The Model row offers **Hermes default** and IDs returned
   by Hermes' `/v1/models` endpoint. Choose an explicit listed ID if the
   default does not work with your Hermes version.

Notchfolk reads `API_SERVER_KEY` from `~/.hermes/.env` (or `HERMES_API_KEY` from
the `notchfolkd` environment) and connects to
`http://127.0.0.1:8642/v1`. For a Hermes somewhere else (a VPS on a
tailnet, for example), Assistant Settings has a Hermes endpoint and API key
row: what they hold is kept in `~/.config/notchfolk/ai.env` (the same private file
the OpenAI-compatible API uses) as `NOTCHFOLK_HERMES_URL` and `HERMES_API_KEY`,
and overrides the local file and the environment. Keep such a Hermes on a
private network (Tailscale, SSH tunnel) or behind TLS: its API exposes its
full toolset, including the terminal of the machine it runs on.

`notchfolkd --doctor` reports Hermes as configured when it finds the API key. That
check does not verify that the gateway is currently reachable. If Notchfolk reports
an API connection error, check that `hermes gateway` is still running, that it
uses port 8642, and that the key in `~/.hermes/.env` matches the running
gateway.

### What Notchfolk and Hermes each handle

Notchfolk sends a streaming OpenAI-compatible Chat Completions request to Hermes,
with Notchfolk's assistant instructions and the selected model. The response text
is parsed as Notchfolk's answer format. Desktop actions in that answer use Notchfolk's
own action and approval flow. Notchfolk stores up to 60 recent conversation
messages in `~/.local/share/notchfolk/assistant-history-hermes.json`, with
user-only file permissions, so the conversation can continue after a restart.
This uses Notchfolk's default data directory; an XDG data-directory override moves
the file with it.

Hermes keeps its own identity, personality, memory, skills, and native tools.
Notchfolk does not replace them with its own persona: instead of "You are Notchfolk…",
Hermes is told only what Notchfolk is (a notch on the user's desktop), the
actions and tools it can reach through it, and how an answer is shaped
(`crates/notchfolk-daemon/src/assistant/mod.rs`, `notchfolk_bridge`). Notchfolk's
Assistant Settings custom instructions are not sent to Hermes either;
configure Hermes' persona in its own SOUL/BOOT files.
Native Hermes tool calls run in the Hermes API server under Hermes' own tool
and approval settings. Notchfolk relays Hermes' API-side approval events: when a
guarded tool asks (`approval.request` on the stream), Notchfolk shows its existing
agent Allow / Deny alert and posts the answer to Hermes' runs API
(`once`, `always` where Hermes offers it, or `deny`); an unanswered or
dismissed alert denies. Notchfolk shows Hermes' tool-progress events
(`hermes.tool.progress`) as activity in the island. Notchfolk's desktop-action
approval covers the actions in Notchfolk's own answer format; it does not change
Hermes' tool configuration. Multi-step tool use, default-model handling, and
cancellation still need live acceptance against the installed Hermes version.

## Oh My Pi inside Notchfolk

Install and configure `omp` with the provider and model you want. Select Oh My
Pi in Assistant Settings; the model is optional, and an empty field uses
OMP's configured default. Notchfolk starts OMP in RPC mode, appends Notchfolk
instructions, and stores the conversation under Notchfolk's private data
directory.

Current source keeps OMP's native tools, extensions, skills, and rules. Notchfolk
starts OMP with `--mode rpc-ui` so its native-tool approval requests can reach
Notchfolk. OMP's `Allow tool: ...` approval dialog is shown through Notchfolk's existing
agent permission alert, and Allow / Deny is sent back to OMP. If Notchfolk cannot
show the alert or it is dismissed, Notchfolk denies the request. Notchfolk runs its OMP
session with `--approval-mode always-ask`: read-only tools can run, and
write/exec calls request approval. A session-only config overlay changes an
explicit `bash: allow` to `prompt`, while preserving `deny` rules. The user's
global OMP config is not edited. Other explicit per-tool policies stay in
effect, and other interactive extension dialogs are cancelled. Notchfolk actions
requested in the in-app assistant use Notchfolk's action path. Notchfolk-managed
`SOUL.md`, `USER.md`, and `MEMORY.md` for OMP are part of the target but are
not implemented yet. OMP v18.12.5 is installed on the audited machine. A
direct prompt to its configured default returned text, and a one-prompt RPC
round trip emitted `ready`, opened a session, streamed text deltas, and ended
with `prompt_result` `completed`. A bash-only RPC check also executed a fixed
`printf` command, emitted OMP's tool start/end events, and returned the output.
This verifies the configured provider, RPC framing, and that native tools stay
enabled under the OMP permission settings; it does not verify a request
through `notchfolkd` and the GNOME UI. The full
`omp models` catalog took about 27 seconds to list, so Assistant Settings now
uses OMP's configured default or an optional manual `provider/model` ID instead
of fetching the entire catalog automatically.

The approval bridge uses OMP's `rpc-ui` event protocol and the same Notchfolk alert
path used for Claude Code and Codex. The RPC event format was captured with a
temporary OMP bash prompt rule and a harmless `printf` command. The code path
and session-only approval policy are implemented, but the actual GNOME alert
click-through has not yet been verified in a live shell session.

## External AI agents through MCP

Use Notchfolk's optional MCP server when an agent outside Notchfolk needs Notchfolk data or
actions. Configure the MCP endpoint and any required bearer token in that
agent's own MCP settings (for example, in Claude Code, Antigravity, or Codex).
Notchfolk does not configure the external agent's model, provider, or API key.

The current server exposes only a selected set of tools. The target is to
expose all Notchfolk data and actions, including useful filtered reads such as
calories for a requested date. See [MCP server setup and current coverage](mcp.md).

## Scope note

OpenAI-compatible BYOK and other coding-agent adapters exist in source, but
they are not target runtimes for the assistant built into Notchfolk. Use the
existing external agent harness for those providers and connect it to Notchfolk
through MCP when it needs Notchfolk capabilities.

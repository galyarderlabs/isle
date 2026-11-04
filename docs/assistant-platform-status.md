# Notchfolk Assistant and MCP: Implementation Audit

Audited 2026-10-29 against [assistant-platform-plan.md](assistant-platform-plan.md).
The in-app assistant and the external MCP server have different jobs and are
tracked separately here. A provider or tool existing in source does not mean
its user-facing flow is complete.

## In-app assistant: source that exists

- Provider selection and backend dispatch currently include Hermes, Oh My Pi
  (OMP), OpenAI-compatible BYOK, and other coding-agent CLIs. The target for
  the Notchfolk assistant is Hermes and OMP; BYOK and the other provider adapters
  are existing source, not required work for this target.
- Hermes uses Notchfolk's OpenAI-compatible streaming client against Hermes'
  local API server at `http://127.0.0.1:8642/v1`. The key comes from
  `~/.hermes/.env` (`API_SERVER_KEY`) or `HERMES_API_KEY`; `NOTCHFOLK_HERMES_URL`
  can override the endpoint. Settings offers Hermes default and IDs read from
  `/v1/models`. Hermes owns provider setup, identity, personality, durable
  memory, and native tools. Notchfolk stores up to 60 recent messages in its
  private `assistant-history-hermes.json` file.
- Hermes setup steps are documented in
  [assistant-runtimes.md](assistant-runtimes.md). Notchfolk's doctor check only
  confirms that it found a key; it does not ping the gateway. Hermes' API
  server grants access to its full toolset, so the documented setup keeps it
  on the default loopback address and uses a key.
- The source parses both text deltas and Hermes' named SSE events. `hermes.tool.progress`
  frames with `status: running` are shown as activity in the island
  ("Hermes is using …"), and `approval.request` frames for a flagged command
  are relayed through Notchfolk's existing agent Allow / Deny alert: the chosen
  answer is posted back to `POST /v1/runs/{run_id}/approval` with Hermes'
  choices (`once`, `always` where Hermes offers it, or `deny`); an unanswered,
  dismissed, or failed alert denies, so a Hermes tool never waits past Notchfolk.
  The answer's own actions still use Notchfolk's own permission path. Hermes-native
  tool execution runs in the Hermes gateway under Hermes' tool settings; Notchfolk
  relays its approval requests but does not change them. No live Hermes API
  round trip, multi-step native-tool flow, Notchfolk action, cancellation, or
  approval acceptance was performed during this audit.
- OMP has an RPC adapter, streaming/progress handling, session persistence,
  and model-catalog lookup code. Current source keeps OMP's native tools,
  extensions, skills, and rules.
- The installed OMP CLI is v18.12.5. A minimal prompt to OMP's configured
  default returned the expected text after about 57 seconds. A separate
  one-prompt RPC run emitted `ready`, opened a session, streamed text deltas,
  and ended with `prompt_result` `completed` after about 53 seconds. This
  verifies the configured provider and RPC framing used by Notchfolk. A bash-only
  RPC check executed a fixed `printf` command, emitted tool start/end events,
  and returned the output; OMP's native tools are enabled under its own
  permissions. These checks have not exercised `notchfolkd` and the GNOME UI.
- Running `omp models` locally took about 27 seconds. Settings no longer
  requests OMP's entire catalog synchronously; it offers OMP's configured
  default and an optional manual model ID.
- Notchfolk's current doctor check only confirms that the OMP executable exists;
  OMP owns credentials for its many provider options. The source now reports
  that OMP manages provider setup instead of claiming the binary is signed in.
- Notchfolk starts OMP with `--mode rpc-ui --approval-mode always-ask`. Read-only
  calls can run; write/exec calls use the existing Notchfolk Allow / Deny alert
  used for Claude Code and Codex, and Notchfolk returns the choice to OMP. A
  session-only overlay changes an explicit `bash: allow` to `prompt` and
  preserves explicit `deny` rules. Other explicit per-tool policies remain
  active. The user's global OMP config is not edited. An unavailable or
  dismissed alert denies the request; unrelated interactive extension
  dialogs are cancelled.
- The installed OMP global config reports `tools.approvalMode: yolo` and
  `tools.approval.bash: allow`; the Notchfolk-launched session now overrides these
  at runtime only, without editing that config. A temporary OMP bash `prompt`
  rule and harmless `printf` command produced an
  `extension_ui_request` (`select`, title `Allow tool: bash`, options `Approve`
  and `Deny`). The corresponding response uses `extension_ui_response` with
  the selected option. This confirms the RPC bridge format. It does not verify
  the alert click-through in a live GNOME Shell session.
- The in-app assistant has an Notchfolk answer/action loop that executes Notchfolk
  actions. The assistant does not use the external MCP server as its internal
  tool interface.
- Assistant Settings can select providers and models, enter a model ID, and
  edit one Custom instructions value. It does not manage OMP's
  `SOUL.md`, `USER.md`, or `MEMORY.md` files.

## External MCP server: source that exists

- A separate optional FastMCP HTTP service exposes a selected set of Notchfolk
  tools to external MCP clients. This is the MCP integration for agents
  outside Notchfolk; the external agent is configured in its own MCP settings.
- The current service has loopback defaults, per-client scopes, and write
  allowlists.
- The MCP tool surface now covers tasks, Notchfolk calendar events and desktop
  calendars with meeting links, reminders, calorie entries, water, prayer
  schedule/location, media (players, lyrics, seek, controls), focus state and
  controls, app usage, system state, AI router stats, AI plan usage, the file
  shelf, clipboard history by search, and opening HTTP(S) URLs through the
  desktop portal. Date-filtered calendar, calorie and water reads, per-client
  key generation (Settings → MCP and `notchfolk-mcp-keygen`), and a Cloudflare
  tunnel (`notchfolk-mcp-tunnel`, with automatic DNS setup) are implemented.
- None of this expanded surface is live-verified yet against a real session
  bus or an external MCP client; that acceptance run remains open, as do
  notifications, calls, and other desktop actions outside the tool list.

## Remaining gaps against the target

- **MCP coverage and queries:** inventory all Notchfolk features and expose them
  through external MCP tools. Include useful structured reads, such as getting
  calories for a specified date or date range, alongside writes and controls.
- **External client setup:** document the MCP endpoint and credentials for
  external clients. Their MCP configuration belongs in Claude Code,
  Antigravity, Codex, or the relevant agent harness, not in Notchfolk's assistant
  provider settings.
- **OMP acceptance:** verify an in-app model response, Notchfolk action execution,
  continuation, cancellation, failures, and an actual Allow / Deny click for
  a native OMP tool approval in GNOME Shell.
- **Hermes acceptance:** verify multi-step in-app requests, Notchfolk action
  execution, continuation, cancellation, and errors against the local API.
- **OMP context:** add Notchfolk-managed `SOUL.md`, `USER.md`, and `MEMORY.md` for
  OMP. Keep Hermes' own personality and memory intact.

Direct OpenAI-compatible BYOK fields in Notchfolk Assistant Settings and an
internal MCP tool-call path are not target requirements. Existing BYOK source
is available for review, but external agents should use their own harness and
connect to Notchfolk through the MCP server.

See [assistant-platform-tasks.md](assistant-platform-tasks.md) for remaining
work and acceptance criteria. No live provider or external MCP-client
acceptance was performed in this audit.

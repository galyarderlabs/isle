# Notchfolk Assistant and MCP: Target Architecture

This document describes two separate products: the assistant that runs inside
Notchfolk, and the MCP server that lets AI agents outside Notchfolk use Notchfolk's features.
Current implementation is in [assistant-platform-status.md](assistant-platform-status.md);
remaining work is in [assistant-platform-tasks.md](assistant-platform-tasks.md).

## 1. Assistant inside Notchfolk

The in-app assistant is Notchfolk's own harness. Its supported runtimes are Hermes
Agent and Oh My Pi (OMP). Notchfolk owns provider selection, its UI, Notchfolk-specific
context, the actions it executes, and the permissions for those actions.

- **Hermes Agent:** connect to Hermes through its local API. Keep Hermes'
  identity, personality, memory, and native runtime behavior. Notchfolk handles
  Notchfolk-owned desktop actions through its own action and approval path. Hermes'
  native tools execute inside its gateway and use Hermes' permissions; their
  approval requests are not currently bridged into Notchfolk's alert UI.
- **Oh My Pi:** connect through OMP's RPC interface. Keep OMP's native coding
  tools and runtime behavior. OMP can use Notchfolk's in-app action path for
  Notchfolk-owned actions; those actions still go through Notchfolk's permissions.
  OMP's configured default model is the normal choice; allow a manual
  `provider/model` override without loading the full model catalog in Settings.
  Native tools that need approval must keep their approval behavior; never
  silently enable auto-approval to make headless calls pass.

The target is a reliable multi-step assistant flow for both runtimes, including
model selection where available, visible progress, action results,
continuation, cancellation, and useful errors.

Direct OpenAI-compatible BYOK is not a target provider for the in-app
assistant. AI agents that use OpenAI-compatible models can use their existing
harness and connect to Notchfolk through the external MCP server described below.
Existing provider code outside Hermes and OMP is not part of this target.

## 2. MCP for AI agents outside Notchfolk

The MCP server is an external interface to Notchfolk's features. It wraps Notchfolk's
data and actions as MCP tools so agents such as Claude Code, Antigravity, or
Codex can call them. Configure the MCP endpoint and credentials in the external agent's own
MCP settings. This is not an internal tool protocol for the assistant inside
Notchfolk, and the external agent's model or API key is not configured in Notchfolk.

The MCP surface must cover all Notchfolk pages and capabilities, including useful
ways to query their data. For example, an agent must be able to request food
entries for a specific date or date range and receive the matching records;
it must not only be able to add a food entry. Inventory every existing Notchfolk
read and action, including tasks, reminders, calendar, focus, media, prayer,
calories, water, clipboard/shelf, notifications, calls, usage, and any other
Notchfolk-owned capability. Define typed inputs and results for each operation.

Apply per-client scopes and write allowlists. Read-only data queries should be
available under read scopes; writes and sensitive desktop actions need the
appropriate authorization and audit trail. If a capability cannot be exposed,
record its specific technical or security reason and provide the closest
useful supported operation where possible.

The MCP server and the in-app assistant may call the same underlying Notchfolk
services, but they remain separate interfaces with separate clients and setup.
Do not require the in-app Hermes or OMP adapter to speak MCP.

## Notchfolk-managed context for OMP

Notchfolk manages three context files for OMP, the non-Hermes runtime in the
in-app assistant:

- `SOUL.md`: personality and working style.
- `USER.md`: stable information and preferences about the user.
- `MEMORY.md`: durable facts and decisions to carry between conversations.

Make these editable in Assistant Settings and include them in OMP sessions.
Hermes continues to manage its own personality and memory. Conversation
transcripts are separate from these context files. Define file locations,
size limits, and safe memory updates before allowing automatic writes.

## Settings boundaries

Assistant Settings configure the in-app Hermes or OMP runtime, model where
available, and Notchfolk-managed OMP context. MCP client setup belongs in the
external agent's own MCP settings; Notchfolk's MCP server configuration covers its
endpoint, authentication, and permissions. Do not add an OpenAI-compatible
provider, base-URL field, or model API-key field to Assistant Settings for
this target.

## Security and reliability

- Keep Hermes and OMP secrets out of logs and diagnostics; preserve their
  runtime-specific secure configuration.
- Keep the Hermes API server bound to loopback and require its API key: the
  endpoint grants access to Hermes' full native toolset.
- Keep sensitive D-Bus caller checks in place.
- Default the MCP server to loopback. Require authentication for non-loopback
  binds and use per-client scopes and write allowlists.
- Apply read, data-write, and sensitive-action permissions consistently to
  MCP calls. Do not expose unrestricted D-Bus operations as tools.
- Verify invalid inputs, date filters, authorization failures, tool errors,
  cancellation, continuation, and service restarts.

## Acceptance

1. Hermes and OMP each complete a multi-step request in the Notchfolk assistant UI
   and route Notchfolk-owned actions through Notchfolk's permission path.
2. An external MCP client can discover and use tools for every inventoried
   Notchfolk capability, with useful read queries as well as actions. For example,
   a date-filtered calorie query returns the matching data.
3. An external agent connects using its own MCP settings and receives results
   from Notchfolk; its model/provider credentials remain in that agent's harness.
4. MCP read scopes, write allowlists, authentication, and audit behavior are
   verified for local and authenticated network access.
5. OMP receives Notchfolk-managed `SOUL.md`, `USER.md`, and `MEMORY.md`; Hermes'
   own identity and memory remain untouched.

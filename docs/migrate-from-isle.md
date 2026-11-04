# Migrating from Isle to Notchfolk

Notchfolk still installs under Isle's names (see the README note); the release that
renames the runtime identifiers ships `notchfolk-migrate-from-isle.sh` with the
package, and that script carries a user's Isle state over in one run.

## The identifiers it maps

The constants at the top of the script are the one place the names live; the
runtime rebrand must match them (and any change there is a one-line edit here):

| | Isle | Notchfolk |
|---|---|---|
| Extension UUID | `isle@zickrian.github.io` | `io.galyarderlabs.notchfolk` |
| Settings schema | `org.gnome.shell.extensions.isle` | `org.gnome.shell.extensions.notchfolk` |
| D-Bus names | `io.github.zickrian.isle.*` | `io.galyarderlabs.notchfolk.*` |
| Keyring service | `io.github.zickrian.isle` | `io.galyarderlabs.notchfolk` |
| Services | `isled.service`, `isle-mcp.service` | `notchfolkd.service`, `notchfolk-mcp.service` |
| Folders | `~/.local/share/isle`, `~/.config/isle`, `~/.cache/isle`, `~/.local/state/isle` | the same with `notchfolk` |

## What it does, in order

1. **Settings.** Every key of the old schema is read and written to the new one,
   only when the new schema has the same key with the same type; anything
   renamed or dropped stays with the old schema's file instead of guessing.
2. **Services.** The old user units are stopped, disabled and removed (backed
   up), so no two daemons answer at once; the new units start only if the
   rebranded build already installed them.
3. **D-Bus activation.** The old service files are removed once the rebranded
   ones exist, so nothing starts the old binary again.
4. **State.** The data, config, cache, state and doc folders move (rename, not
   copy): tasks, calendar entries, reminders, water and calorie history, app
   usage, clipboard history with its images, the shelf, `mcp.env`,
   `mcp-clients.json` and the MCP OAuth database all go with them.
5. **Keyring.** The router's password, proxy key and session cookies are read
   from the old service name and stored under the new one (`secret-tool`;
   without it they stay put and signing in again replaces them).
6. **The old extension.** Only reported: the rebranded installer owns disabling
   and removing it, so the script never touches another installation's files.

## Running it

```sh
notchfolk-migrate-from-isle.sh --dry-run   # report what would happen
notchfolk-migrate-from-isle.sh              # migrate
```

It refuses to run before the rebranded build is installed (its compiled settings
schema or its daemon is the check), moves state instead of copying it so nothing
is left behind under the old names, skips anything whose target already exists
(`--force` overrides, after checking both), and keeps removed service files in
`~/.local/state/notchfolk-migration-backup-<timestamp>` as the rollback path —
putting one back and `systemctl --user daemon-reload` restores it.

The env var names inside `mcp.env` are rewritten to the rebranded service's
(`ISLE_MCP_*` to `NOTCHFOLK_MCP_*`), with the file's values kept as they are and a
copy in the backup folder.

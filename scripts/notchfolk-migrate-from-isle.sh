#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# One-time migration of a user's Isle state to Notchfolk, for the release that renames
# the runtime identifiers (see docs/rebrand-plan.md). Safe to run once after the
# rebranded build is installed; --dry-run reports without touching anything.
set -euo pipefail

OLD_APP=isle
NEW_APP=notchfolk
OLD_UUID="isle@zickrian.github.io"
NEW_UUID="io.galyarderlabs.notchfolk"
OLD_SCHEMA="org.gnome.shell.extensions.isle"
NEW_SCHEMA="org.gnome.shell.extensions.notchfolk"
OLD_BUS="io.github.zickrian.isle"
NEW_BUS="io.galyarderlabs.notchfolk"
NEW_DAEMON=notchfolkd
OLD_UNITS=(isled.service isle-mcp.service)
NEW_UNITS=(notchfolkd.service notchfolk-mcp.service)

DRY_RUN=false
FORCE=false
BACKUP=

CONFIG_HOME="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
STATE_HOME="${XDG_STATE_HOME:-$HOME/.local/state}"
CACHE_HOME="${XDG_CACHE_HOME:-$HOME/.cache}"

usage() {
    cat <<'EOF'
notchfolk-migrate-from-isle [--dry-run] [--force]

Migrate an existing Isle installation's settings, data, credentials and services
to Notchfolk's identifiers. Run it after installing the rebranded build; it moves
(mostly renames) state instead of copying it, so nothing is left behind running
under the old names. --dry-run reports what would happen; --force also moves state
when the target already has some.
EOF
}

say() { echo "$*"; }
run() {
    if $DRY_RUN; then
        say "would: $*"
    else
        say "$ $*"
        "$@"
    fi
}

# Moves a directory, refusing when the target already holds something (data would mix).
move_dir() {
    local from=$1 to=$2
    [ -d "$from" ] || return 0
    if [ -d "$to" ] && [ -n "$(ls -A "$to" 2>/dev/null)" ] && ! $FORCE; then
        say "skip: $to already exists and is not empty (use --force to move anyway; check both first)"
        return 0
    fi
    run mkdir -p "$(dirname "$to")"
    run mv "$from" "$to"
}

# Saves a small file (a systemd or D-Bus service definition) before removing it.
stow() {
    local file=$1
    [ -f "$file" ] || return 0
    if ! $DRY_RUN; then
        mkdir -p "$BACKUP"
        cp -p "$file" "$BACKUP/"
    fi
    say "backed up $file to $BACKUP"
    run rm "$file"
}

for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        --force) FORCE=true ;;
        -h|--help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done

BACKUP="$STATE_HOME/$NEW_APP-migration-backup-$(date +%Y%m%d-%H%M%S)"

# The rebranded build must already be there: migrating without it only breaks the
# running Isle. Its compiled settings schema is the clearest sign of it.
if ! $DRY_RUN; then
    if ! gsettings list-schemas 2>/dev/null | grep -Fxq "$NEW_SCHEMA" && ! command -v "$NEW_DAEMON" >/dev/null; then
        say "The Notchfolk build is not installed yet (no $NEW_SCHEMA schema, no $NEW_DAEMON)."
        say "Install it first, then run this again; --dry-run reports without that check."
        exit 1
    fi
fi

# Settings: every key's value and type, read before write, never touching keys Notchfolk dropped.
if gsettings list-schemas 2>/dev/null | grep -Fxq "$NEW_SCHEMA" \
    && gsettings list-schemas 2>/dev/null | grep -Fxq "$OLD_SCHEMA"; then
    new_types=$(gsettings list-recursively "$NEW_SCHEMA")
    while IFS=' ' read -r key type value; do
        [ -n "$key" ] || continue
        want=$(printf '%s\n' "$new_types" | awk -v k="$key" '$1 == k {print $2}')
        if [ "$want" = "$type" ]; then
            run gsettings set "$NEW_SCHEMA" "$key" "$value"
        else
            say "skip: $key is ${want:-absent} in $NEW_SCHEMA; its value stays with the old schema"
        fi
    done <<EOF
$(gsettings list-recursively "$OLD_SCHEMA")
EOF
else
    say "skip: settings need both schemas present ($OLD_SCHEMA and $NEW_SCHEMA)"
fi

# Services: the old names stop first, so no two daemons answer at once; the new
# ones start only if the rebranded build already installed their unit files.
for unit in "${OLD_UNITS[@]}"; do
    if systemctl --user cat "$unit" >/dev/null 2>&1; then
        run systemctl --user disable --now "$unit"
        stow "$CONFIG_HOME/systemd/user/$unit"
    fi
done
if ! $DRY_RUN; then
    systemctl --user daemon-reload 2>/dev/null || true
fi
for unit in "${NEW_UNITS[@]}"; do
    if [ -f "$CONFIG_HOME/systemd/user/$unit" ]; then
        run systemctl --user enable --now "$unit"
    else
        say "note: $unit is not installed; the rebranded build's installer starts it"
    fi
done

# D-Bus activation: the old service files would start the old binary again, so they
# go once the rebranded build has placed its own.
for name in Daemon ShelfWindow; do
    old_service="$DATA_HOME/dbus-1/services/$OLD_BUS.$name.service"
    new_service="$DATA_HOME/dbus-1/services/$NEW_BUS.$name.service"
    if [ -f "$old_service" ]; then
        if [ -f "$new_service" ]; then
            stow "$old_service"
        else
            say "skip: $old_service kept; $NEW_BUS.$name.service is not installed yet"
        fi
    fi
done

# State: data, config, cache and the MCP OAuth database move to the new names.
move_dir "$DATA_HOME/$OLD_APP" "$DATA_HOME/$NEW_APP"
move_dir "$CONFIG_HOME/$OLD_APP" "$CONFIG_HOME/$NEW_APP"
move_dir "$CACHE_HOME/$OLD_APP" "$CACHE_HOME/$NEW_APP"
move_dir "$STATE_HOME/$OLD_APP" "$STATE_HOME/$NEW_APP"
move_dir "$DATA_HOME/doc/$OLD_APP" "$DATA_HOME/doc/$NEW_APP"

# Keyring: the router's password, proxy key and session cookies, read from the old
# service name and stored under the new one. rust-keyring names each entry's account
# `username`, so the accounts come from the search's attribute lines and each secret
# is looked up plainly (search output is not the plaintext). Without secret-tool they
# stay put and signing in again replaces them.
if command -v secret-tool >/dev/null; then
    while IFS= read -r account; do
        [ -n "$account" ] || continue
        secret=$(secret-tool lookup service "$OLD_BUS" username "$account" 2>/dev/null) || continue
        printf '%s' "$secret" | run secret-tool store --label 'Notchfolk router' service "$NEW_BUS" username "$account"
        run secret-tool clear service "$OLD_BUS" username "$account"
    done < <(
        secret-tool search --all service "$OLD_BUS" 2>/dev/null \
            | awk -F' = ' '/^attribute\.username / { print $2 }'
    )
else
    say "skip: secret-tool is not installed; the router's stored password and sessions stay under $OLD_BUS"
    say "      (re-signing in stores them again; or install libsecret-tools and rerun before removing Isle)"
fi

# The old extension: only reported. The rebranded installer owns disabling and
# removing it, so this never touches another installation's files.
if [ -d "$DATA_HOME/gnome-shell/extensions/$OLD_UUID" ]; then
    say "note: the $OLD_UUID extension folder is still installed;"
    say "      after the rebranded extension ($NEW_UUID) is on, remove the old one from the Extension Manager"
fi

if $DRY_RUN; then
    say "done (dry run: nothing was changed)."
else
    say "done. backups: $BACKUP (delete once Notchfolk looks right)"
fi

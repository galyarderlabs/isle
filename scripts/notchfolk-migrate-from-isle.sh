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
under the old names. --dry-run reports what would happen; --force merges state
into a target that already has some.
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
# The installer itself creates the target first (install.lock, the doc copy), so a
# target holding only the installer's own files is set aside and the move goes ahead.
# --force merges instead: the old contents are copied over the new ones, which a plain
# mv would nest into the target instead of replacing it.
move_dir() {
    local from=$1 to=$2
    [ -d "$from" ] || return 0
    if [ -d "$to" ] && [ -n "$(ls -A "$to" 2>/dev/null)" ]; then
        if $FORCE; then
            say "note: $to already exists; --force merges $from into it (old files win)"
            run cp -a "$from/." "$to/" && run rm -rf "$from"
            return 0
        fi
        local only_installers=true entry
        for entry in "$to"/*; do
            [ -e "$entry" ] || continue
            case "${entry##*/}" in
                install.lock | README.md | mcp.md) ;;
                *) only_installers=false; break ;;
            esac
        done
        if $only_installers; then
            say "note: $to only holds the installer's own files; setting them aside for the move"
            if ! $DRY_RUN; then mkdir -p "$BACKUP"; fi
            for entry in "$to"/*; do
                [ -e "$entry" ] && run mv "$entry" "$BACKUP/"
            done
            run rmdir "$to"
        else
            say "skip: $to already exists and is not empty (use --force to move anyway; check both first)"
            return 0
        fi
    fi
    run mkdir -p "$(dirname "$to")"
    run mv "$from" "$to"
}

# Moves one entry into place, recursing into directories that exist on both sides.
# On a file collision the old copy wins (it holds the user's history); a file and a
# directory sharing a name are left alone instead of guessed.
merge_entry() {
    local from=$1 to=$2
    if [ -d "$from" ] && [ -d "$to" ]; then
        local entry base
        for entry in "$from"/* "$from"/.[!.]*; do
            [ -e "$entry" ] || continue
            base=$(basename "$entry")
            merge_entry "$entry" "$to/$base"
        done
        return 0
    fi
    if [ -d "$from" ] || [ -d "$to" ]; then
        say "warning: $from and $to are different kinds of entry; leaving both alone"
        return 0
    fi
    if [ -e "$to" ]; then
        say "note: $to already exists; the old copy replaces it"
    fi
    run mv "$from" "$to"
}

# Moves the data folder. Unlike move_dir it merges when the target already exists:
# the installer (and setup-mcp) already placed fresh code there (mcp/, mcp-venv/),
# and a plain rename would replace that code with the old Isle copy (whose server
# still calls itself "Isle"). So the old code and venv stay behind and only the
# user's data files move.
move_data_dir() {
    local from=$1 to=$2
    [ -d "$from" ] || return 0
    if [ ! -d "$to" ]; then
        run mkdir -p "$(dirname "$to")"
        run mv "$from" "$to"
        return 0
    fi
    say "note: $to already exists; merging the old data in"
    local entry base
    for entry in "$from"/* "$from"/.[!.]*; do
        [ -e "$entry" ] || continue
        base=$(basename "$entry")
        case "$base" in
            mcp|mcp-venv)
                say "note: leaving $from/$base behind (code/venv — the new install owns it)"
                continue ;;
        esac
        merge_entry "$entry" "$to/$base"
    done
    run find "$from" -mindepth 1 -type d -empty -delete 2>/dev/null || true
    if $DRY_RUN; then
        say "would: rmdir $from (once its data moved)"
    elif [ -z "$(ls -A "$from" 2>/dev/null)" ]; then
        run rmdir "$from"
    else
        say "note: $from still holds mcp/ and mcp-venv/; delete it by hand once Notchfolk looks right"
    fi
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
    # The installer removes the old extension folder, which takes the old compiled
    # schema with it — so the schema check above usually fails on a real machine,
    # while the values still sit in dconf. dconf needs no schema, so copy them raw.
    say "note: settings schemas are not both present ($OLD_SCHEMA and $NEW_SCHEMA); trying dconf instead"
    if command -v dconf >/dev/null \
        && dconf dump "/org/gnome/shell/extensions/$OLD_APP/" | grep -q '^[^[]'; then
        if $DRY_RUN; then
            say "would: copy settings with dconf dump/load (no schemas needed)"
        else
            dconf dump "/org/gnome/shell/extensions/$OLD_APP/" | dconf load "/org/gnome/shell/extensions/$NEW_APP/"
        fi
    else
        say "skip: no settings stored under $OLD_SCHEMA (or dconf is not installed)"
    fi
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
# The data folder merges (see move_data_dir); the rest are plain renames.
move_data_dir "$DATA_HOME/$OLD_APP" "$DATA_HOME/$NEW_APP"
move_dir "$CONFIG_HOME/$OLD_APP" "$CONFIG_HOME/$NEW_APP"
move_dir "$CACHE_HOME/$OLD_APP" "$CACHE_HOME/$NEW_APP"
move_dir "$STATE_HOME/$OLD_APP" "$STATE_HOME/$NEW_APP"
move_dir "$DATA_HOME/doc/$OLD_APP" "$DATA_HOME/doc/$NEW_APP"

# The MCP env file's variable names belonged to the old name; the rebranded service reads
# NOTCHFOLK_MCP_*. Its values are kept as they are.
env_file="$CONFIG_HOME/$NEW_APP/mcp.env"
if [ -f "$env_file" ]; then
    if ! $DRY_RUN; then
        mkdir -p "$BACKUP"
        cp -p "$env_file" "$BACKUP/"
    fi
    run sed -i -e 's/ISLE_MCP_/NOTCHFOLK_MCP_/g' "$env_file"
fi

# Keyring: the router's password, proxy key and session cookies, read from the old
# service name and stored under the new one. rust-keyring names each entry's account
# `username`, so the accounts come from the search's attribute lines and each secret
# is looked up plainly (search output is not the plaintext). Without secret-tool they
# stay put and signing in again replaces them.
if command -v secret-tool >/dev/null; then
    accounts=$(secret-tool search --all service "$OLD_BUS" 2>/dev/null \
        | awk -F' = ' '/^attribute\.username / { print $2 }')
    if [ -z "$accounts" ]; then
        say "note: no keyring entries under $OLD_BUS; nothing to move"
    else
        say "found $(printf '%s\n' "$accounts" | grep -c .) keyring entries under $OLD_BUS"
        while IFS= read -r account; do
            [ -n "$account" ] || continue
            secret=$(secret-tool lookup service "$OLD_BUS" username "$account" 2>/dev/null) || continue
            printf '%s' "$secret" | run secret-tool store --label 'Notchfolk router' service "$NEW_BUS" username "$account" \
                || { say "warning: could not store $account under $NEW_BUS; keeping the old entry"; continue; }
            # The old entry is only cleared once the new one reads back with the
            # same value, so a failed store never loses the credential.
            if $DRY_RUN; then
                run secret-tool clear service "$OLD_BUS" username "$account"
            elif [ "$(secret-tool lookup service "$NEW_BUS" username "$account" 2>/dev/null)" = "$secret" ]; then
                run secret-tool clear service "$OLD_BUS" username "$account"
            else
                say "warning: $account does not read back under $NEW_BUS; keeping the old entry"
            fi
        done <<< "$accounts"
    fi
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

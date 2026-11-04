#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Installs the optional Python runtime for Notchfolk's MCP HTTP server.
set -euo pipefail

ROOT="${XDG_DATA_HOME:-$HOME/.local/share}/notchfolk"
PYTHON="${PYTHON:-python3}"
REQUIREMENTS="$ROOT/mcp/requirements.txt"
[ -f "$REQUIREMENTS" ] || REQUIREMENTS="$(dirname "$0")/../mcp/requirements.txt"
[ -f "$REQUIREMENTS" ] || REQUIREMENTS="/usr/share/notchfolk/mcp/requirements.txt"
command -v "$PYTHON" >/dev/null || { echo "Install Python 3 first." >&2; exit 1; }
"$PYTHON" -c 'import sys; raise SystemExit(sys.version_info < (3, 11))' || {
    echo "Notchfolk MCP requires Python 3.11 or newer." >&2
    exit 1
}
"$PYTHON" -m venv "$ROOT/mcp-venv"
"$ROOT/mcp-venv/bin/python" -m pip install --upgrade pip
"$ROOT/mcp-venv/bin/python" -m pip install -r "$REQUIREMENTS"
echo "Notchfolk MCP runtime is ready. Enable notchfolk-mcp.service to start the HTTP server."

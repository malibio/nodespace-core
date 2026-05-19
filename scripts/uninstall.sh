#!/bin/sh
# NodeSpace uninstaller — POSIX sh
# Stops the daemon, removes binaries and service files.
# User data at ~/.nodespace/database/ is PRESERVED.
set -e

# ── Constants ──────────────────────────────────────────────────────────────────
INSTALL_DIR="$HOME/.nodespace/bin"
SOCKET_PATH="$HOME/.nodespace/daemon.sock"
PLIST_PATH="$HOME/Library/LaunchAgents/app.nodespace.daemon.plist"
SYSTEMD_SERVICE="$HOME/.config/systemd/user/nodespace.service"
MCP_CONFIG_MACOS="$HOME/Library/Application Support/Claude/claude_desktop_config.json"
MCP_CONFIG_LINUX="$HOME/.config/Claude/claude_desktop_config.json"
SKILL_DIR="$HOME/.claude/skills/nodespace"
LAUNCHD_LABEL="app.nodespace.daemon"

OS=$(uname -s)

# ── Stop daemon ───────────────────────────────────────────────────────────────
printf 'Stopping nodespaced...\n'
case "$OS" in
    Darwin)
        launchctl bootout "gui/$(id -u)/$LAUNCHD_LABEL" 2>/dev/null || true
        ;;
    Linux)
        systemctl --user stop nodespace 2>/dev/null || true
        systemctl --user disable nodespace 2>/dev/null || true
        ;;
esac

# ── Remove service files ──────────────────────────────────────────────────────
case "$OS" in
    Darwin)
        if [ -f "$PLIST_PATH" ]; then
            rm -f "$PLIST_PATH"
            printf 'Removed %s\n' "$PLIST_PATH"
        fi
        ;;
    Linux)
        if [ -f "$SYSTEMD_SERVICE" ]; then
            rm -f "$SYSTEMD_SERVICE"
            systemctl --user daemon-reload 2>/dev/null || true
            printf 'Removed %s\n' "$SYSTEMD_SERVICE"
        fi
        ;;
esac

# ── Remove binaries ───────────────────────────────────────────────────────────
if [ -d "$INSTALL_DIR" ]; then
    rm -f "$INSTALL_DIR/nodespaced" "$INSTALL_DIR/nodespace"
    # Remove the bin dir only if empty
    rmdir "$INSTALL_DIR" 2>/dev/null || true
    printf 'Removed binaries from %s\n' "$INSTALL_DIR"
fi

# ── Remove socket ─────────────────────────────────────────────────────────────
if [ -e "$SOCKET_PATH" ]; then
    rm -f "$SOCKET_PATH"
    printf 'Removed socket %s\n' "$SOCKET_PATH"
fi

# ── Remove Claude Code skill ───────────────────────────────────────────────────
if [ -d "$SKILL_DIR" ]; then
    rm -rf "$SKILL_DIR"
    printf 'Removed Claude Code skill at %s\n' "$SKILL_DIR"
fi

# ── Remove MCP entry from Claude Desktop config ────────────────────────────────
case "$OS" in
    Darwin) _mcp_cfg="$MCP_CONFIG_MACOS" ;;
    *)      _mcp_cfg="$MCP_CONFIG_LINUX" ;;
esac

if [ -f "$_mcp_cfg" ] && grep -q '"nodespace"' "$_mcp_cfg" 2>/dev/null; then
    # Count total mcpServers entries to gauge complexity
    _entry_count=$(grep -c '"command"' "$_mcp_cfg" 2>/dev/null || printf '0')

    # For complex configs, recommend the CLI which uses serde_json for safe removal.
    if [ "$_entry_count" -le 5 ]; then
        # Conservative block removal: delete the nodespace key through its closing brace.
        # Handles the common case where the block spans ~4 lines.
        awk '
            /^[[:space:]]*"nodespace"[[:space:]]*:/ { skip=1 }
            skip && /\}[[:space:]]*,?[[:space:]]*$/ { skip=0; next }
            !skip { print }
        ' "$_mcp_cfg" > "$_mcp_cfg.tmp" && mv "$_mcp_cfg.tmp" "$_mcp_cfg"
        printf 'Removed nodespace MCP entry from %s\n' "$_mcp_cfg"
    else
        printf 'Note: %s has many entries.\n' "$_mcp_cfg"
        printf 'Run `nodespace uninstall` for safe JSON-aware removal, or remove manually:\n'
        printf '  "nodespace": { "command": "nodespace", "args": ["mcp"] }\n'
    fi
fi

# ── Done ──────────────────────────────────────────────────────────────────────
printf '\nNodeSpace uninstalled. Your data at ~/.nodespace/database/ has been preserved.\n'

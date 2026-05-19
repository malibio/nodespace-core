#!/bin/sh
# NodeSpace installer — POSIX sh
# Installs nodespaced + nodespace CLI to ~/.nodespace/bin/ and registers
# the background daemon with launchd (macOS) or systemd (Linux).
#
# Trust model: SHA256 checksums verify that downloaded binaries match the
# release artifacts, but do not prove publisher identity. A compromised GitHub
# release could replace both binaries and checksums simultaneously. GPG
# signature verification is tracked in a follow-on issue.
set -e

# ── Constants ──────────────────────────────────────────────────────────────────
INSTALL_DIR="$HOME/.nodespace/bin"
LOG_DIR="$HOME/.nodespace/logs"
DB_PATH="$HOME/.nodespace/database/nodespace"
SOCKET_PATH="$HOME/.nodespace/daemon.sock"
GITHUB_API="https://api.github.com/repos/NodeSpaceAI/nodespace-core/releases/latest"
GITHUB_DL="https://github.com/NodeSpaceAI/nodespace-core/releases/download"
LAUNCHD_LABEL="app.nodespace.daemon"
PLIST_PATH="$HOME/Library/LaunchAgents/app.nodespace.daemon.plist"
SYSTEMD_SERVICE="$HOME/.config/systemd/user/nodespace.service"
MCP_CONFIG_MACOS="$HOME/Library/Application Support/Claude/claude_desktop_config.json"
MCP_CONFIG_LINUX="$HOME/.config/Claude/claude_desktop_config.json"
SKILL_PATH="$HOME/.claude/skills/nodespace/SKILL.md"

# ── Utilities ─────────────────────────────────────────────────────────────────
die() { printf '\nError: %s\n' "$*" >&2; exit 1; }

check_cmd() {
    command -v "$1" >/dev/null 2>&1
}

require_cmd() {
    check_cmd "$1" || die "'$1' is required but not found. Please install it and retry."
}

sha256_verify() {
    _file="$1"
    _expected="$2"
    if check_cmd sha256sum; then
        _actual=$(sha256sum "$_file" | awk '{print $1}')
    elif check_cmd shasum; then
        _actual=$(shasum -a 256 "$_file" | awk '{print $1}')
    else
        die "No sha256 tool found (tried sha256sum and shasum)"
    fi
    if [ "$_actual" != "$_expected" ]; then
        die "SHA256 mismatch for $1\n  expected: $_expected\n  got:      $_actual"
    fi
}

# ── Platform detection ────────────────────────────────────────────────────────
detect_triple() {
    _os=$(uname -s)
    _arch=$(uname -m)
    case "$_os" in
        Darwin)
            case "$_arch" in
                arm64)  printf 'aarch64-apple-darwin' ;;
                x86_64) printf 'x86_64-apple-darwin' ;;
                *)      die "Unsupported macOS architecture: $_arch" ;;
            esac
            ;;
        Linux)
            case "$_arch" in
                aarch64) printf 'aarch64-unknown-linux-gnu' ;;
                x86_64)  printf 'x86_64-unknown-linux-gnu' ;;
                *)       die "Unsupported Linux architecture: $_arch" ;;
            esac
            ;;
        *)
            die "Unsupported platform: $_os. NodeSpace supports macOS and Linux."
            ;;
    esac
}

OS=$(uname -s)
TRIPLE=$(detect_triple)

# ── Dependency checks ─────────────────────────────────────────────────────────
require_cmd curl

# ── Fetch latest release version ─────────────────────────────────────────────
printf 'Fetching latest release...\n'
VERSION=$(curl -fsSL "$GITHUB_API" \
    | grep '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
[ -n "$VERSION" ] || die "Could not determine latest release version"
printf 'Installing NodeSpace %s (%s)\n' "$VERSION" "$TRIPLE"

# ── Download binaries and checksums ───────────────────────────────────────────
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

DL_BASE="$GITHUB_DL/$VERSION"

printf 'Downloading nodespaced...\n'
curl -fsSL -o "$TMP_DIR/nodespaced" "$DL_BASE/nodespaced-$TRIPLE"

printf 'Downloading nodespace...\n'
curl -fsSL -o "$TMP_DIR/nodespace" "$DL_BASE/nodespace-$TRIPLE"

printf 'Downloading SHA256SUMS...\n'
curl -fsSL -o "$TMP_DIR/SHA256SUMS" "$DL_BASE/SHA256SUMS"

# ── Verify checksums ──────────────────────────────────────────────────────────
printf 'Verifying checksums...\n'
DAEMON_SUM=$(grep "nodespaced-$TRIPLE" "$TMP_DIR/SHA256SUMS" | awk '{print $1}')
CLI_SUM=$(grep "nodespace-$TRIPLE" "$TMP_DIR/SHA256SUMS" | awk '{print $1}')

[ -n "$DAEMON_SUM" ] || die "No checksum found for nodespaced-$TRIPLE in SHA256SUMS"
[ -n "$CLI_SUM" ]    || die "No checksum found for nodespace-$TRIPLE in SHA256SUMS"

sha256_verify "$TMP_DIR/nodespaced" "$DAEMON_SUM"
sha256_verify "$TMP_DIR/nodespace"  "$CLI_SUM"
printf 'Checksums OK\n'

# ── Install binaries ──────────────────────────────────────────────────────────
mkdir -p "$INSTALL_DIR" "$LOG_DIR" "$(dirname "$DB_PATH")"

cp "$TMP_DIR/nodespaced" "$INSTALL_DIR/nodespaced"
cp "$TMP_DIR/nodespace"  "$INSTALL_DIR/nodespace"
chmod 755 "$INSTALL_DIR/nodespaced" "$INSTALL_DIR/nodespace"
printf 'Binaries installed to %s\n' "$INSTALL_DIR"

# ── Service registration ───────────────────────────────────────────────────────

# macOS: launchd
install_launchd() {
    mkdir -p "$(dirname "$PLIST_PATH")"
    cat > "$PLIST_PATH" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>app.nodespace.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>$INSTALL_DIR/nodespaced</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>NODESPACED_SOCKET</key>
        <string>$SOCKET_PATH</string>
        <key>NODESPACED_DB_PATH</key>
        <string>$DB_PATH</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>$LOG_DIR/nodespaced.log</string>
    <key>StandardErrorPath</key>
    <string>$LOG_DIR/nodespaced-error.log</string>
</dict>
</plist>
PLIST

    _uid=$(id -u)
    _target="gui/$_uid"

    # Check registration state explicitly before choosing the launchctl verb.
    # Using bootstrap failure as a proxy is unreliable — it also fails on
    # permission errors and malformed plists.
    if launchctl print "$_target/$LAUNCHD_LABEL" >/dev/null 2>&1; then
        # Already registered — kickstart to pick up updated binary
        if launchctl kickstart -k "$_target/$LAUNCHD_LABEL" 2>/dev/null; then
            printf 'launchd agent restarted\n'
        else
            printf 'Note: launchctl kickstart failed; daemon will start on next login\n'
        fi
    else
        if launchctl bootstrap "$_target" "$PLIST_PATH" 2>/dev/null; then
            printf 'launchd agent bootstrapped\n'
        else
            printf 'Note: launchctl bootstrap failed; daemon may start on next login\n'
        fi
    fi
}

# Linux: systemd user service
install_systemd() {
    mkdir -p "$(dirname "$SYSTEMD_SERVICE")"
    cat > "$SYSTEMD_SERVICE" <<UNIT
[Unit]
Description=NodeSpace daemon
After=network.target

[Service]
Type=simple
ExecStart=$INSTALL_DIR/nodespaced
Environment=NODESPACED_SOCKET=$SOCKET_PATH
Environment=NODESPACED_DB_PATH=$DB_PATH
StandardOutput=append:$LOG_DIR/nodespaced.log
StandardError=append:$LOG_DIR/nodespaced-error.log
Restart=on-failure

[Install]
WantedBy=default.target
UNIT

    systemctl --user daemon-reload
    systemctl --user enable --now nodespace
    printf 'systemd user service enabled and started\n'
}

case "$OS" in
    Darwin) install_launchd ;;
    Linux)  install_systemd ;;
esac

# ── Wait for daemon socket ─────────────────────────────────────────────────────
printf 'Waiting for daemon...'
_ticks=0
while [ $_ticks -lt 10 ]; do
    if [ -S "$SOCKET_PATH" ]; then
        printf ' ready\n'
        break
    fi
    # Each tick is 0.5s (or 1s on systems without fractional sleep).
    # 10 ticks × 0.5s = 5s total wait.
    if sleep 0.5 2>/dev/null; then
        _ticks=$((_ticks + 1))
    else
        sleep 1
        _ticks=$((_ticks + 2))
    fi
done
if [ ! -S "$SOCKET_PATH" ] && [ $_ticks -ge 10 ]; then
    printf ' (timeout — daemon may still be starting)\n'
fi

# ── PATH ──────────────────────────────────────────────────────────────────────
printf '\nAdd nodespace to your PATH? [Y/n] '
read -r _ans
case "${_ans:-Y}" in
    [Yy]*)
        _path_line='export PATH="$HOME/.nodespace/bin:$PATH"'
        _rc_updated=0
        for _rc in "$HOME/.zshrc" "$HOME/.bash_profile"; do
            if [ -f "$_rc" ]; then
                if grep -qF '.nodespace/bin' "$_rc" 2>/dev/null; then
                    printf '  %s already configured\n' "$_rc"
                else
                    printf '\n# NodeSpace\n%s\n' "$_path_line" >> "$_rc"
                    printf '  Added to %s\n' "$_rc"
                fi
                _rc_updated=1
            fi
        done
        if [ "$_rc_updated" -eq 0 ]; then
            printf '  No ~/.zshrc or ~/.bash_profile found.\n'
            printf '  Add manually: %s\n' "$_path_line"
        fi
        ;;
    *) printf '  Skipped. Run: export PATH="$HOME/.nodespace/bin:$PATH"\n' ;;
esac

# ── MCP (Claude Desktop) ──────────────────────────────────────────────────────
printf '\nConnect NodeSpace to Claude Desktop (MCP)? [Y/n] '
read -r _ans
case "${_ans:-Y}" in
    [Yy]*)
        case "$OS" in
            Darwin) _mcp_cfg="$MCP_CONFIG_MACOS" ;;
            *)      _mcp_cfg="$MCP_CONFIG_LINUX" ;;
        esac

        _mcp_entry='"nodespace": {\n    "command": "nodespace",\n    "args": ["mcp"]\n  }'

        if [ -f "$_mcp_cfg" ]; then
            if grep -q '"nodespace"' "$_mcp_cfg" 2>/dev/null; then
                printf '  nodespace MCP entry already present in %s\n' "$_mcp_cfg"
            elif grep -q '"mcpServers"' "$_mcp_cfg" 2>/dev/null; then
                # Insert after the opening brace of mcpServers
                # Use awk for reliable multi-line insertion
                awk '
                    /"mcpServers"/ { print; getline; print; printf "    %s,\n", entry; next }
                    { print }
                ' entry="$_mcp_entry" "$_mcp_cfg" > "$_mcp_cfg.tmp" \
                    && mv "$_mcp_cfg.tmp" "$_mcp_cfg"
                printf '  Added nodespace MCP entry to %s\n' "$_mcp_cfg"
            else
                printf '  Could not locate mcpServers in %s\n' "$_mcp_cfg"
                printf '  Add manually:\n    %s\n' "$_mcp_entry"
            fi
        else
            mkdir -p "$(dirname "$_mcp_cfg")"
            cat > "$_mcp_cfg" <<JSON
{
  "mcpServers": {
    "nodespace": {
      "command": "nodespace",
      "args": ["mcp"]
    }
  }
}
JSON
            printf '  Created %s\n' "$_mcp_cfg"
        fi
        ;;
    *) printf '  Skipped\n' ;;
esac

# ── Claude Code skill ─────────────────────────────────────────────────────────
printf '\nAdd NodeSpace skill to Claude Code? [Y/n] '
read -r _ans
case "${_ans:-Y}" in
    [Yy]*)
        mkdir -p "$(dirname "$SKILL_PATH")"
        cat > "$SKILL_PATH" <<'SKILL'
# nodespace

Use this skill to interact with the local NodeSpace knowledge graph.

## When to use
- When asked to store, search, or retrieve information in NodeSpace
- When the user says "save this to NodeSpace" or "search NodeSpace for..."

## Usage
Run `nodespace node list` to list nodes, `nodespace search <query>` to search.
SKILL
        printf '  Skill written to %s\n' "$SKILL_PATH"
        ;;
    *) printf '  Skipped\n' ;;
esac

# ── Success summary ───────────────────────────────────────────────────────────
printf '\n'
printf '✓ NodeSpace installed to ~/.nodespace/bin/\n'
if [ -S "$SOCKET_PATH" ]; then
    printf '✓ nodespaced is running\n'
else
    printf '✓ nodespaced registered (will start on next login if not running yet)\n'
fi
printf '✓ Try: nodespace node list\n'
printf '\n'

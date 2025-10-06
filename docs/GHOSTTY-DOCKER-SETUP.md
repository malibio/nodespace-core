# Using Ghostty Terminal with NodeSpace Docker Containers

This guide explains how to use Ghostty terminal with NodeSpace development containers for an optimal development experience.

## Overview

**Architecture:**
- **Host (macOS)**: Runs Ghostty terminal emulator
- **Container (Docker)**: Runs development environment with Chrome, Zsh, Claude Code

**Why this approach:**
- Ghostty is a native macOS terminal (GPU-accelerated, Ghostty-specific features)
- Running GUI apps in Docker adds significant complexity (X11 forwarding, VNC, etc.)
- This setup provides the best of both worlds: Ghostty's features + isolated development environment

## Setup

### 1. Install Ghostty on macOS

```bash
# Using Homebrew
brew install --cask ghostty

# Or download from https://ghostty.org
```

### 2. Configure Ghostty for Docker Integration

Create or edit `~/.config/ghostty/config`:

```bash
# Ghostty Configuration for Docker Development

# Font configuration
font-family = "JetBrains Mono"
font-size = 14

# Color scheme
theme = "gruvbox-dark"

# Shell integration
shell-integration = true
shell-integration-features = cursor,sudo,title

# Window configuration
window-padding-x = 4
window-padding-y = 4
window-decoration = true

# Performance
performance-mode = true

# Keybindings for container management
keybind = ctrl+shift+d=new_window:docker run -it --rm claude-code-dev:authenticated
```

### 3. Build Enhanced Container

```bash
cd /Users/malibio/nodespace/nodespace-core

# Build container with Chrome and enhanced shell
bun run container:build

# Run container in Ghostty
bun run container:run
```

## Usage Patterns

### Starting a New Development Session

```bash
# In Ghostty terminal, start a new container
bun run container:run

# Inside container, you'll see:
# 🚀 Claude Code Development Container Ready!
# Repository: /workspace/nodespace-core
# Available tools: git, rust, bun, claude, gh, tmux, zsh
# ✅ Chrome: Google Chrome 131.x.x.x (for chrome-devtools MCP)

# Start working
git checkout -b feature/my-feature
~/.local/bin/claude
```

### Using Chrome DevTools MCP

Inside the container, Claude Code can use the chrome-devtools MCP:

```bash
# Chrome is pre-installed and configured
echo $CHROME_BIN
# /usr/bin/google-chrome-stable

# Verify Chrome works
google-chrome-stable --version
# Google Chrome 131.x.x.x

# Claude Code will automatically detect Chrome for MCP operations
~/.local/bin/claude
# Now you can use chrome-devtools commands!
```

### Multiple Containers in Ghostty

Ghostty supports tabs and splits, perfect for parallel development:

```bash
# Open new Ghostty tab (Cmd+T)
# Run second container
bun run container:run

# Split Ghostty window (Cmd+D / Cmd+Shift+D)
# Run third container for different feature
bun run container:run
```

### Terminal Multiplexing with tmux

For complex workflows inside a single container:

```bash
# Inside container, start tmux
tmux new-session -A -s development

# tmux is pre-configured with:
# - 256 color support
# - Mouse support
# - Automatic window renaming
# - Aggressive resize for better multi-client support
```

## Advanced Configuration

### Custom Ghostty Profiles for Containers

Create container-specific Ghostty profiles in `~/.config/ghostty/config`:

```bash
# Profile: nodespace-dev
[profile:nodespace-dev]
font-size = 12
theme = "nord"
title = "NodeSpace Development"
working-directory = /Users/malibio/nodespace/nodespace-core

# Use with: ghostty --profile=nodespace-dev
```

### Shell Integration Features

The container's Zsh shell includes:

- **Oh My Zsh**: Framework with themes and plugins
- **zsh-autosuggestions**: Fish-like autosuggestions
- **zsh-syntax-highlighting**: Syntax highlighting for commands
- **Plugins**: git, rust, bun, docker

### Persistent Container Workflow

For long-running development sessions:

```bash
# 1. Start persistent container (survives exit)
bun run container:run:setup

# 2. Work inside container (authenticate, configure, etc.)
~/.local/bin/claude auth login
git config --global user.name "Your Name"

# 3. Exit container (Ctrl+D)
exit

# 4. Save container state
bun run container:save

# 5. Future sessions will use authenticated image
# Update CONTAINER_IMAGE in scripts/container.ts to:
# const CONTAINER_IMAGE = "claude-code-dev:authenticated";
```

## Troubleshooting

### Terminal Size Issues

If the terminal size is incorrect:

```bash
# Inside container, manually resize
resize_terminal 120 40

# Or from host (while container is running)
bun run container:resize
```

### Chrome Not Working

```bash
# Verify Chrome installation
google-chrome-stable --version

# Check environment variables
echo $CHROME_BIN
echo $PUPPETEER_EXECUTABLE_PATH

# Test Chrome headless mode
google-chrome-stable --headless --disable-gpu --dump-dom https://example.com
```

### Zsh Not Loading Properly

```bash
# Force reload Zsh configuration
source ~/.zshrc

# Or switch back to bash temporarily
bash
```

## Container Features Summary

**Development Tools:**
- ✅ Rust (rustc, cargo, cargo-watch)
- ✅ Bun runtime and package manager
- ✅ Node.js (latest LTS)
- ✅ Claude Code CLI
- ✅ GitHub CLI (gh)
- ✅ Git with sensible defaults

**Shell Environment:**
- ✅ Zsh with Oh My Zsh
- ✅ Auto-suggestions and syntax highlighting
- ✅ tmux for terminal multiplexing
- ✅ Enhanced tools: ripgrep, fd, bat, htop

**Browser Automation:**
- ✅ Google Chrome (stable)
- ✅ Configured for headless mode
- ✅ Ready for chrome-devtools MCP
- ✅ Puppeteer-compatible

**Terminal Features:**
- ✅ 256 color support
- ✅ Automatic terminal sizing
- ✅ Manual resize functions
- ✅ tmux integration

## Workflow Comparison

### Without Ghostty (Standard Docker)

```bash
# Basic terminal experience
docker run -it claude-code-dev:authenticated
# ❌ Limited terminal features
# ❌ No GPU acceleration
# ❌ Basic keybindings
```

### With Ghostty (Recommended)

```bash
# Rich terminal experience
ghostty
bun run container:run
# ✅ GPU-accelerated rendering
# ✅ Advanced keybindings
# ✅ Native macOS integration
# ✅ Tabs and splits
# ✅ Shell integration
```

## Best Practices

1. **Use Ghostty tabs** for multiple containers (parallel features)
2. **Use tmux splits** for multiple terminals in one container (same feature, different panes)
3. **Save authenticated containers** to avoid re-authenticating
4. **Configure Ghostty profiles** for different project contexts
5. **Use shell aliases** in Zsh for common commands

## Example: Full Development Session

```bash
# 1. Open Ghostty
open -a Ghostty

# 2. Start container
cd /Users/malibio/nodespace/nodespace-core
bun run container:run

# 3. Inside container - verify environment
google-chrome-stable --version  # ✅ Chrome ready
zsh --version                    # ✅ Zsh 5.9
claude --version                 # ✅ Claude Code ready

# 4. Start feature work
git checkout -b feature/my-feature
~/.local/bin/claude

# 5. Use chrome-devtools MCP in Claude Code
# Claude can now execute browser automation tasks!

# 6. Open new Ghostty tab (Cmd+T) for parallel feature
# Repeat steps 2-5 with different branch

# 7. Clean up when done
exit
bun run container:stop
```

## Resources

- [Ghostty Terminal](https://ghostty.org)
- [Oh My Zsh](https://ohmyz.sh)
- [Chrome DevTools Protocol](https://chromedevtools.github.io/devtools-protocol/)
- [Claude Code MCP](https://docs.anthropic.com/en/docs/agents-and-tools/mcp)

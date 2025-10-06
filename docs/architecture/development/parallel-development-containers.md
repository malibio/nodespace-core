# Parallel Development with Docker Containers

## Quick Start Guide

This guide provides step-by-step instructions for using Docker containers for parallel NodeSpace development.

---

## Prerequisites

Ensure you have a container runtime installed:

```bash
# Check if Colima is running (recommended for macOS)
colima status

# If not running, start Colima:
colima start

# OR use Docker Desktop
# Verify Docker is running:
docker info
```

---

## Step 1: Build the Development Container

The container includes:
- ✅ **Chromium Browser** (ARM64 compatible, for chrome-devtools MCP)
- ✅ **Zsh with Oh My Zsh** (officially supported by Claude Code)
- ✅ **Rust, Bun, Node.js, Git, GitHub CLI**
- ✅ **Enhanced shell utilities** (ripgrep, fd, bat, htop, vim, nano)
- ✅ **tmux** for terminal multiplexing

```bash
cd /Users/malibio/nodespace/nodespace-core

# Build the container (first time only, ~5-10 minutes)
bun run container:build
```

**Expected output:**
```
🔨 Building Claude Code development container with docker...
[... build progress ...]
✅ Container image built successfully!
```

---

## Step 2: Run Your First Container

```bash
# Start a new development container
bun run container:run
```

**What happens:**
1. Container starts with isolated Ubuntu environment
2. Automatically clones `nodespace-core` repository inside container
3. Drops you into Zsh shell with Oh My Zsh
4. Container gets unique name: `nodespace-dev-[timestamp]`

**You'll see:**
```
🚀 Claude Code Development Container Ready!
Repository: /workspace/nodespace-core
Available tools: git, rust, bun, claude (~/.local/bin/claude), gh, tmux, zsh
Terminal size: 120x40
Terminal: zsh with Oh My Zsh (tmux available manually)

✅ Chromium: Chromium 131.x.x.x (for chrome-devtools MCP)

Quick start:
  git checkout -b feature/container-[timestamp]
  ~/.local/bin/claude

root@nodespace-dev-[timestamp]:/workspace/nodespace-core#
```

---

## Step 3: Authenticate Claude Code (One-Time Setup)

**Inside the container**, authenticate Claude Code:

```bash
# Authenticate with Claude
~/.local/bin/claude auth login

# Follow the prompts to complete authentication
```

**After authentication**, you can save this state:

```bash
# Exit the container (Ctrl+D or type exit)
exit

# Save the authenticated container as a new image
bun run container:save
```

**Update the default image** in `scripts/container.ts`:
- Change line 13 from: `const CONTAINER_IMAGE = "claude-code-dev:authenticated";`
- To use the saved image name

Now all future containers will start pre-authenticated!

---

## Step 4: Start Working on a Feature

**Inside the container:**

```bash
# 1. Create your feature branch
git checkout -b feature/my-awesome-feature

# 2. Start Claude Code
~/.local/bin/claude

# 3. Now you can use all Claude Code features including:
#    - chrome-devtools MCP (Chromium is installed and configured)
#    - Status line (Zsh is fully compatible)
#    - All normal development tools
```

---

## Step 5: Parallel Development (Multiple Containers)

To work on multiple features simultaneously:

### Terminal 1: Feature A
```bash
cd /Users/malibio/nodespace/nodespace-core
bun run container:run

# Inside container:
git checkout -b feature/feature-a
~/.local/bin/claude
```

### Terminal 2: Feature B (New terminal window/tab)
```bash
cd /Users/malibio/nodespace/nodespace-core
bun run container:run

# Inside container:
git checkout -b feature/feature-b
~/.local/bin/claude
```

### Terminal 3: Feature C (Another new terminal)
```bash
cd /Users/malibio/nodespace/nodespace-core
bun run container:run

# Inside container:
git checkout -b feature/feature-c
~/.local/bin/claude
```

Each container is **completely isolated**:
- Separate Git working directory
- Independent Claude Code session
- Own branch checkout
- No conflicts between containers

---

## Managing Containers

### List Running Containers
```bash
bun run container:list
```

**Output:**
```
📋 NodeSpace Development Containers (Colima):
CONTAINER ID   NAMES                      STATUS        IMAGE
abc123         nodespace-dev-1234567890   Up 2 hours    claude-code-dev:authenticated
def456         nodespace-dev-0987654321   Up 1 hour     claude-code-dev:authenticated
```

### Stop Containers

```bash
# Stop all NodeSpace containers
bun run container:stop

# Stop specific container
bun run container:stop nodespace-dev-1234567890
```

### Clean Up

```bash
# List containers to see what exists
bun run container:list

# Remove old/stopped containers and rebuild
docker system prune -f

# Rebuild container with latest changes
bun run container:build
```

---

## Terminal Features

### Zsh with Oh My Zsh

The container shell includes:

```bash
# Auto-suggestions (type to see suggestions)
git sta[tab]  # autocompletes to 'git status'

# Syntax highlighting
ls -la  # Valid command shows in green
lsss    # Invalid command shows in red

# Git integration
# Prompt shows current branch and git status
~/nodespace-core (feature/my-feature) $
```

### tmux (Optional Terminal Multiplexing)

For advanced users who want multiple panes in one container:

```bash
# Start tmux session
tmux new-session -A -s development

# Split horizontally: Ctrl+B then "
# Split vertically: Ctrl+B then %
# Navigate panes: Ctrl+B then arrow keys
# Detach: Ctrl+B then D
# Reattach: tmux attach -t development
```

---

## Using chrome-devtools MCP

Chromium is pre-installed and configured for MCP use:

```bash
# Verify Chromium is working
chromium-browser --version
# Chromium 131.x.x.x

# Check environment variables
echo $CHROME_BIN
# /usr/bin/chromium-browser

# Claude Code will automatically use Chromium for MCP
~/.local/bin/claude

# Now you can ask Claude to use browser automation:
# "Open https://example.com and take a screenshot"
# "Navigate to the GitHub page and click the Issues tab"
```

---

## Troubleshooting

### Terminal Size Issues

If the terminal looks cramped:

```bash
# Inside container, manually resize:
resize_terminal 150 50

# Or from host (with container running):
bun run container:resize
```

### Container Won't Start

```bash
# Check if container runtime is running
colima status

# If stopped, start it:
colima start

# Check Docker info
docker info
```

### Chromium Not Found

```bash
# Inside container, verify installation:
which chromium-browser
# /usr/bin/chromium-browser

# Test Chromium:
chromium-browser --version

# Check environment:
echo $CHROME_BIN
echo $PUPPETEER_EXECUTABLE_PATH
```

### Repository Not Cloned

If the repository didn't clone automatically:

```bash
# Inside container:
cd /workspace
git clone https://github.com/malibio/nodespace-core.git
cd nodespace-core
```

### Zsh Not Loading Properly

```bash
# Reload Zsh configuration:
source ~/.zshrc

# Or temporarily switch to bash:
bash
```

---

## Workflow Best Practices

### 1. One Feature Per Container

```bash
# ✅ Good: Each container focuses on one feature
Container 1: feature/add-search
Container 2: feature/fix-bug-123
Container 3: feature/refactor-ui

# ❌ Avoid: Switching branches in same container
# This defeats the purpose of isolation
```

### 2. Save Authenticated State

```bash
# After initial setup, save your authenticated container:
bun run container:save

# Update CONTAINER_IMAGE in scripts/container.ts
# Now you don't need to re-authenticate every time
```

### 3. Clean Up Regularly

```bash
# Stop containers when done:
bun run container:stop

# Containers with --rm flag auto-delete on exit
# But check for lingering containers:
bun run container:list
```

### 4. Use Host for Git Operations

For complex git operations (merges, rebases, etc.), consider using your host machine:

```bash
# On host (macOS):
cd /Users/malibio/nodespace/nodespace-core
git fetch --all
git rebase main

# Then in container:
git pull
```

### 5. Terminal Multiplexing Strategy

- **Multiple containers**: Different features (recommended)
- **tmux in one container**: Same feature, different views
- **Don't mix**: Avoid both simultaneously (complexity)

---

## Advanced: Persistent Containers

For long-running sessions that survive restarts:

```bash
# Start persistent container (no --rm flag)
bun run container:run:setup

# Work inside container...
# Exit doesn't delete container

# Later, re-attach:
docker exec -it nodespace-dev-[timestamp] /bin/zsh

# When completely done:
bun run container:stop nodespace-dev-[timestamp]
docker rm nodespace-dev-[timestamp]
```

---

## Summary: Complete Workflow

```bash
# 1. Build container (first time only)
cd /Users/malibio/nodespace/nodespace-core
bun run container:build

# 2. Start container
bun run container:run

# 3. Inside container: authenticate (first time)
~/.local/bin/claude auth login
exit

# 4. Save authenticated state
bun run container:save

# 5. Future sessions: just run and work
bun run container:run
# Inside: git checkout -b feature/X && ~/.local/bin/claude

# 6. Parallel development: open new terminal, repeat step 5

# 7. Clean up when done
bun run container:stop
```

---

## Container Features Summary

| Feature | Status | Details |
|---------|--------|---------|
| **Shell** | ✅ Zsh 5.8.1 | With Oh My Zsh framework |
| **Browser** | ✅ Chromium | ARM64 compatible, MCP ready |
| **Runtime** | ✅ Bun | Package manager and runtime |
| **Language** | ✅ Rust | cargo, rustc, cargo-watch |
| **VCS** | ✅ Git + GitHub CLI | Pre-configured |
| **AI** | ✅ Claude Code CLI | Installed at ~/.local/bin/claude |
| **Multiplexer** | ✅ tmux | Pre-configured with sane defaults |
| **Utilities** | ✅ ripgrep, fd, bat, htop, vim | Modern CLI tools |
| **Terminal Support** | ✅ 256 color, emojis | Full compatibility |
| **Status Line** | ✅ Compatible | Zsh officially supported |

---

## Resources

- [Container Management Script](../scripts/container.ts)
- [Dockerfile](../Dockerfile.dev)
- [Claude Code Documentation](https://docs.claude.com/en/docs/claude-code)
- [Oh My Zsh](https://ohmyz.sh)
- [Chromium Browser](https://www.chromium.org)

---

**Questions or issues?** Check the troubleshooting section or file an issue in the repository.

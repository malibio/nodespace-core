# Development Environment Setup

## Overview

This guide covers setting up a complete NodeSpace development environment with all required services, AI models, and development tools for optimal productivity.

## Prerequisites

### System Requirements

**Minimum:**
- 16GB RAM (for AI model + development environment)
- 50GB free disk space (for AI models and development databases)
- macOS 12+, Windows 10+, or Ubuntu 20.04+

**Recommended:**
- 32GB RAM (for comfortable development with large AI models)
- 100GB free disk space
- macOS with Apple Silicon (optimal Metal GPU performance) or NVIDIA GPU on other platforms

### Required Software

```bash
# Core development tools
rustc 1.80.0+
cargo 1.80.0+
node.js 20.0.0+
npm 9.0.0+

# No additional database systems needed (LanceDB is embedded)

# Platform-specific build tools
# macOS:
xcode-select --install

# Ubuntu/Debian:
sudo apt-get install build-essential pkg-config libssl-dev

# Windows:
# Install Visual Studio Build Tools 2019+
```

## Quick Setup Script

For automated setup on macOS and Linux:

```bash
#!/bin/bash
# setup-dev-environment.sh

set -e

echo "🚀 Setting up NodeSpace development environment..."

# Check system requirements
check_requirements() {
    echo "📋 Checking system requirements..."
    
    # Check RAM
    if [[ $(sysctl -n hw.memsize 2>/dev/null || echo "0") -lt 17179869184 ]]; then
        echo "⚠️  Warning: Less than 16GB RAM detected. AI model performance may be limited."
    fi
    
    # Check disk space
    if [[ $(df -k . | tail -1 | awk '{print $4}') -lt 52428800 ]]; then
        echo "❌ Error: Less than 50GB free disk space. Please free up space."
        exit 1
    fi
    
    echo "✅ System requirements check passed"
}

# Install Rust and Cargo
install_rust() {
    if ! command -v rustc &> /dev/null; then
        echo "🦀 Installing Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source ~/.cargo/env
    else
        echo "✅ Rust already installed: $(rustc --version)"
    fi
    
    # Install required targets
    rustup target add wasm32-unknown-unknown
}

# Install Node.js and npm
install_node() {
    if ! command -v node &> /dev/null; then
        echo "📦 Installing Node.js..."
        if [[ "$OSTYPE" == "darwin"* ]]; then
            brew install node
        elif command -v apt-get &> /dev/null; then
            curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
            sudo apt-get install -y nodejs
        fi
    else
        echo "✅ Node.js already installed: $(node --version)"
    fi
}

# Setup LanceDB directory
setup_lancedb() {
    echo "🗄️  Setting up LanceDB directory..."
    mkdir -p ~/nodespace/data/lance_db
    echo "✅ LanceDB directory ready"
}

# Setup data directories
setup_data_directories() {
    echo "🗄️  Setting up data directories..."
    
    # Create NodeSpace data directories
    mkdir -p ~/nodespace/data/lance_db
    mkdir -p ~/nodespace/data/lance_db_test
    
    echo "✅ Data directories ready"
}

# Download and setup AI models
setup_ai_models() {
    echo "🧠 Setting up AI models..."
    
    # Create models directory
    mkdir -p ~/nodespace/models
    
    # Download Gemma 3n-E4B-it 8B UQFF if not exists
    if [ ! -d ~/nodespace/models/gemma-3n-8b-it-UQFF ]; then
        echo "📥 Downloading Gemma 3n-E4B-it 8B UQFF model..."
        echo "   This may take 10-30 minutes depending on your connection..."
        
        # Install huggingface-cli if needed (requires Python)
        # Note: Only needed for initial model download
        if ! command -v huggingface-cli &> /dev/null; then
            echo "Installing huggingface-cli for model download..."
            pip3 install --user huggingface_hub[cli] || pip install --user huggingface_hub[cli]
        fi
        
        # Download model files
        cd ~/nodespace/models
        huggingface-cli download EricB/gemma-3n-E4B-it-UQFF --local-dir gemma-3n-8b-it-UQFF --local-dir-use-symlinks False
        
        echo "✅ AI model downloaded successfully"
    else
        echo "✅ AI model already available"
    fi
}

# Install development dependencies
install_dev_dependencies() {
    echo "📚 Installing development dependencies..."
    
    # Note: Python tools only needed for initial setup
    # mistral.rs handles model loading natively in Rust
    
    # Development tools
    cargo install cargo-watch cargo-audit sqlx-cli
    npm install -g @tauri-apps/cli
    
    echo "✅ Development dependencies installed"
}

# Clone and setup NodeSpace repository
setup_repository() {
    if [ ! -d "nodespace-core" ]; then
        echo "📦 Setting up NodeSpace repository..."
        # git clone <repository-url> nodespace-core
        # cd nodespace-core
        
        # Install Rust dependencies
        echo "🔧 Installing Rust dependencies..."
        cargo build
        
        # Install Node.js dependencies
        echo "🔧 Installing Node.js dependencies..."
        npm install
        
        echo "✅ Repository setup complete"
    else
        echo "✅ Repository already setup"
    fi
}

# Run setup steps
main() {
    check_requirements
    install_rust
    install_node
    setup_lancedb
    setup_data_directories
    setup_ai_models
    install_dev_dependencies
    setup_repository
    
    echo ""
    echo "🎉 NodeSpace development environment setup complete!"
    echo ""
    echo "Next steps:"
    echo "1. cd nodespace-core"
    echo "2. cargo run  # Start the application"
    echo "3. Open http://localhost:1420 in your browser"
    echo ""
    echo "For development with hot reload:"
    echo "  cargo watch -x run"
    echo "  npm run dev"
}

main "$@"
```

## Manual Setup Instructions

### 1. Rust Development Environment

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Verify installation
rustc --version
cargo --version

# Install required components
rustup component add rustfmt clippy
rustup target add wasm32-unknown-unknown

# Install useful development tools
cargo install cargo-watch    # Auto-rebuild on file changes
cargo install cargo-audit    # Security vulnerability scanning
```

### 2. Node.js Development Environment

```bash
# Install Node.js (via Node Version Manager recommended)
curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
nvm install 20
nvm use 20

# Verify installation
node --version
npm --version

# Install global development tools
npm install -g @tauri-apps/cli  # Tauri CLI
npm install -g typescript       # TypeScript compiler
npm install -g vite            # Build tool
```

### 3. Data Storage Setup

#### LanceDB Configuration

```bash
# Create data directories
mkdir -p ~/nodespace/data/lance_db
mkdir -p ~/nodespace/data/lance_db_test
```

#### Environment Configuration

Create `.env` file in project root:

```bash
# .env
# LanceDB Configuration
LANCE_DB_PATH=/Users/username/nodespace/data/lance_db
TEST_LANCE_DB_PATH=/Users/username/nodespace/data/lance_db_test

# AI Configuration
AI_MODEL_PATH=/Users/username/nodespace/models/gemma-3n-8b-it-UQFF
AI_BACKEND=mistralrs

# Development settings
RUST_LOG=debug
RUST_BACKTRACE=1
```

### 4. AI Model Setup

#### Download Gemma 3n-E4B-it 8B Model

```bash
# Install Hugging Face CLI (Python required for this step only)
pip install huggingface_hub[cli]

# Create models directory
mkdir -p ~/nodespace/models
cd ~/nodespace/models

# Download model (4-6GB download)
# Using EricB's UQFF-optimized version for mistral.rs compatibility
huggingface-cli download EricB/gemma-3n-E4B-it-UQFF \
  --local-dir gemma-3n-8b-it-UQFF \
  --local-dir-use-symlinks False

# Verify download
ls -la gemma-3n-8b-it-UQFF/
```

#### Alternative: Smaller Development Model

For development machines with limited resources:

```bash
# Download smaller 4B parameter model
huggingface-cli download google/gemma-3n-E4B-it-4b \
  --local-dir gemma-3n-4b-it-UQFF \
  --local-dir-use-symlinks False
```

Update `.env` file accordingly:

```bash
AI_MODEL_PATH=/Users/username/nodespace/models/gemma-3n-4b-it-UQFF
```

### 5. Project Setup

```bash
# Clone repository
git clone <repository-url> nodespace-core
cd nodespace-core

# Install Rust dependencies and build
cargo build

# Install Node.js dependencies
npm install

# Run tests to verify setup
cargo test
npm test
```

## Development Workflow

### Starting the Development Environment

```bash
# Terminal 1: Backend with auto-reload
cargo watch -x run

# Terminal 2: Frontend development server
npm run dev

# Data is managed automatically by LanceDB
# No manual database management needed
```

### Environment Configuration

Create `config/development.toml`:

```toml
[app]
name = "NodeSpace"
environment = "development"
debug = true

[server]
host = "127.0.0.1"
port = 1420

[database]
path = "/Users/username/nodespace/data/lance_db"

[ai]
backend = "mistralrs"
model_path = "/Users/username/nodespace/models/gemma-3n-8b-it-UQFF"
max_context_length = 8192
temperature = 0.7
max_tokens = 1024

[logging]
level = "debug"
format = "json"

[cache]
enabled = true
ttl_seconds = 3600
max_size_mb = 512
```

### IDE Configuration

#### VS Code Setup

Create `.vscode/settings.json`:

```json
{
  "rust-analyzer.cargo.features": ["all"],
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.inlayHints.enable": true,
  "svelte.enable-ts-plugin": true,
  "typescript.preferences.importModuleSpecifier": "relative",
  "files.associations": {
    "*.rs": "rust",
    "*.svelte": "svelte"
  },
  "editor.formatOnSave": true,
  "editor.codeActionsOnSave": {
    "source.fixAll": true
  }
}
```

Create `.vscode/extensions.json`:

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "svelte.svelte-vscode",
    "bradlc.vscode-tailwindcss",
    "ms-vscode.vscode-typescript-next",
    "vadimcn.vscode-lldb"
  ]
}
```

#### Rust Analyzer Configuration

Create `.vscode/launch.json` for debugging:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "lldb",
      "request": "launch",
      "name": "Debug NodeSpace",
      "cargo": {
        "args": ["build", "--bin=nodespace-core"],
        "filter": {
          "name": "nodespace-core",
          "kind": "bin"
        }
      },
      "args": [],
      "cwd": "${workspaceFolder}",
      "env": {
        "RUST_LOG": "debug",
        "RUST_BACKTRACE": "1"
      }
    }
  ]
}
```

## Development Scripts

Create `scripts/dev.sh`:

```bash
#!/bin/bash

# Development helper script

case "$1" in
  "start")
    echo "🚀 Starting NodeSpace development environment..."
    tmux new-session -d -s nodespace -x 120 -y 30
    tmux send-keys -t nodespace "cargo watch -x run" Enter
    tmux split-window -h -t nodespace
    tmux send-keys -t nodespace "npm run dev" Enter
    tmux split-window -v -t nodespace
    tmux send-keys -t nodespace "psql nodespace_dev" Enter
    tmux attach-session -t nodespace
    ;;
  "test")
    echo "🧪 Running all tests..."
    cargo test --workspace
    npm test
    ;;
  "lint")
    echo "🔍 Running linters..."
    cargo clippy --all-targets --all-features -- -D warnings
    cargo fmt --check
    npm run lint
    ;;
  "build")
    echo "🔨 Building for production..."
    cargo build --release
    npm run build
    ;;
  "clean")
    echo "🧹 Cleaning build artifacts..."
    cargo clean
    rm -rf node_modules/.cache
    rm -rf dist
    ;;
  "data-reset")
    echo "🗄️  Resetting development data..."
    rm -rf ~/nodespace/data/lance_db/*
    mkdir -p ~/nodespace/data/lance_db
    ;;
  "model-info")
    echo "🧠 AI Model information..."
    ls -la ~/nodespace/models/
    du -sh ~/nodespace/models/*/
    ;;
  *)
    echo "NodeSpace Development Helper"
    echo ""
    echo "Usage: $0 {start|test|lint|build|clean|db-reset|model-info}"
    echo ""
    echo "Commands:"
    echo "  start     - Start development environment with tmux"
    echo "  test      - Run all tests"
    echo "  lint      - Run code linters"
    echo "  build     - Build for production"
    echo "  clean     - Clean build artifacts"
    echo "  db-reset  - Reset development database"
    echo "  model-info- Show AI model information"
    ;;
esac
```

Make it executable:

```bash
chmod +x scripts/dev.sh
```

## Testing the Setup

### 1. Basic Functionality Test

```bash
# Start the application
cargo run

# In another terminal, test the API
curl http://localhost:1420/health

# Expected response:
# {"status": "healthy", "version": "0.1.0"}
```

### 2. AI Integration Test

```bash
# Test AI model loading
cargo run --bin test-ai-model

# Expected output:
# ✅ Model loaded successfully
# ✅ Generated embeddings: [768 dimensions]
# ✅ Generated text response
```

### 3. Database Connection Test

```bash
# Test database connection
sqlx migrate run
cargo test database_connection

# Expected output:
# test database_connection ... ok
```

### 4. Frontend Integration Test

```bash
# Start frontend development server
npm run dev

# Open browser to http://localhost:5173
# Should see NodeSpace interface loading
```

## Troubleshooting

### Common Issues

#### Rust Build Errors

```bash
# Update Rust toolchain
rustup update

# Clear cargo cache
cargo clean

# Check for missing system dependencies
# macOS:
xcode-select --install

# Ubuntu:
sudo apt-get install build-essential pkg-config libssl-dev
```

#### AI Model Loading Issues

```bash
# Check model files exist
ls -la ~/nodespace/models/gemma-3n-8b-it-UQFF/

# Check disk space
df -h ~/nodespace/models/

# Test model files exist
ls -la ~/nodespace/models/gemma-3n-8b-it-UQFF/
echo "Model files should include .safetensors and config files"
```

#### Database Issues

```bash
# Check LanceDB directory exists and is writable
ls -la ~/nodespace/data/lance_db/
touch ~/nodespace/data/lance_db/test_write && rm ~/nodespace/data/lance_db/test_write

# Reset data if corrupted
rm -rf ~/nodespace/data/lance_db/*
mkdir -p ~/nodespace/data/lance_db
```

#### Memory Issues with AI Model

```bash
# Check available memory
free -h  # Linux
vm_stat  # macOS

# Use smaller model for development
export AI_MODEL_PATH=~/nodespace/models/gemma-3n-4b-it-UQFF

# Adjust AI configuration in .env
AI_MAX_CONTEXT_LENGTH=4096
AI_MAX_TOKENS=512
```

### Performance Optimization

#### Development Build Speed

```bash
# Use faster linker (macOS)
export RUSTFLAGS="-C link-arg=-fuse-ld=lld"

# Enable parallel frontend builds
export NODE_OPTIONS="--max-old-space-size=8192"

# Use incremental compilation
export CARGO_INCREMENTAL=1
```

#### AI Model Performance

```bash
# Enable Metal GPU acceleration (macOS)
export PYTORCH_ENABLE_MPS_FALLBACK=1

# Optimize for development
export AI_BATCH_SIZE=1
export AI_PRECISION=fp16
```

## Next Steps

After successful setup:

1. **Explore the Codebase**: Start with `src/main.rs` and `src/lib/components/`
2. **Run Examples**: Check `examples/` directory for sample usage
3. **Read Architecture Docs**: Review `docs/architecture/` for detailed design
4. **Create Your First Extension**: Follow the [Plugin Development Guide](../plugins/development-guide.md)
5. **Join Development**: Review [Testing Strategies](testing-strategies.md) for contribution guidelines

---

This development setup provides a complete environment for building and extending NodeSpace with optimal performance and developer experience.
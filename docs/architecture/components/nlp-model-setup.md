# NLP Model Setup Guide

**Note**: Models are stored in the centralized `~/.nodespace/models/` directory, following the same pattern as the database.

## Required Model: bge-small-en-v1.5

The embedding service requires the `bge-small-en-v1.5` model in ONNX format at:
- **Location**: `~/.nodespace/models/BAAI-bge-small-en-v1.5/`

## For End Users

Models are automatically bundled with the NodeSpace application. No manual setup required.

On first run, the application extracts bundled models to `~/.nodespace/models/`.

## For Developers

### Download Instructions

#### Option 1: Using huggingface-hub CLI (Recommended)

```bash
# Install huggingface-hub
pip install huggingface-hub

# Create directory
mkdir -p ~/.nodespace/models

# Download model files to centralized location
huggingface-cli download BAAI/bge-small-en-v1.5 --local-dir ~/.nodespace/models/BAAI-bge-small-en-v1.5
```

#### Option 2: Convert from PyTorch to ONNX

If the model doesn't have ONNX files, convert them:

```bash
pip install optimum[onnxruntime] transformers

python << EOF
from optimum.onnxruntime import ORTModelForFeatureExtraction
from transformers import AutoTokenizer

# Export to ONNX
model = ORTModelForFeatureExtraction.from_pretrained(
    "BAAI/bge-small-en-v1.5",
    export=True
)

tokenizer = AutoTokenizer.from_pretrained("BAAI/bge-small-en-v1.5")

# Save to centralized models directory
model.save_pretrained("~/.nodespace/models/BAAI-bge-small-en-v1.5")
tokenizer.save_pretrained("~/.nodespace/models/BAAI-bge-small-en-v1.5")
EOF
```

### Expected Directory Structure

After downloading, you should have:

```
~/.nodespace/
├── database/
│   └── nodespace.db             # Database location
└── models/
    └── BAAI-bge-small-en-v1.5/  # Note: "/" replaced with "-"
        ├── model.onnx           # ONNX model file (~130MB)
        ├── tokenizer.json       # Tokenizer configuration
        ├── tokenizer_config.json
        ├── special_tokens_map.json
        └── vocab.txt
```

**Minimum required files:**
- `model.onnx` - The ONNX model
- `tokenizer.json` - Tokenizer configuration

### Verification

After downloading, verify the files exist:

```bash
ls -lh ~/.nodespace/models/BAAI-bge-small-en-v1.5/
```

You should see `model.onnx` (approximately 130MB) and `tokenizer.json`.

### Model Information

- **Model**: BAAI/bge-small-en-v1.5
- **Dimensions**: 384
- **Max Sequence Length**: 512 tokens
- **Size**: ~130MB (ONNX format)
- **License**: MIT
- **Use Case**: Semantic search and similarity matching

### Testing

Run tests to verify the model loads correctly:

```bash
cargo test --features embedding-service --package nodespace-nlp-engine
```

## Build-Time Model Bundling

For CI/CD and releases, models are downloaded during the build process:

```bash
# Download models for bundling (happens automatically during build)
bun run download:models

# Models are downloaded to packages/desktop-app/src-tauri/resources/models/
# Then bundled with the Tauri application
```

## Alternative Models (Future)

This system can support additional models for specialized use cases:

- `nomic-embed-text`: Better code understanding (768 dims)
- `all-MiniLM-L6-v2`: Faster, smaller alternative (384 dims)

Currently, only `bge-small-en-v1.5` is supported.

## Version Control

Model files are large binaries and should not be committed to git. The `.gitignore` excludes:

```
*.onnx
*.bin
*.safetensors
```

Only documentation and configuration files should be committed.

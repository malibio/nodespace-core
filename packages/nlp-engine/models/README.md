# Model Files Directory

This directory contains bundled ONNX models for the NodeSpace NLP Engine.

## Required Model: bge-small-en-v1.5

The embedding service requires the `bge-small-en-v1.5` model in ONNX format.

### Download Instructions

#### Option 1: Using huggingface-hub CLI (Recommended)

```bash
# Install huggingface-hub
pip install huggingface-hub

# Download model files to this directory
cd packages/nlp-engine/models
huggingface-cli download BAAI/bge-small-en-v1.5 --local-dir bge-small-en-v1.5
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

# Save to models directory
model.save_pretrained("packages/nlp-engine/models/bge-small-en-v1.5")
tokenizer.save_pretrained("packages/nlp-engine/models/bge-small-en-v1.5")
EOF
```

### Expected Directory Structure

After downloading, you should have:

```
models/
└── bge-small-en-v1.5/
    ├── model.onnx          # ONNX model file (~130MB)
    ├── tokenizer.json      # Tokenizer configuration
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
ls -lh bge-small-en-v1.5/
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
cd ../..  # Back to workspace root
cargo test --features embedding-service --package nodespace-nlp-engine
```

## Alternative Models (Future)

This directory can support additional models for specialized use cases:

- `nomic-embed-text`: Better code understanding (768 dims)
- `all-MiniLM-L6-v2`: Faster, smaller alternative (384 dims)

Currently, only `bge-small-en-v1.5` is supported.

## gitignore

Model files are large binary files and should not be committed to git. The `.gitignore` should exclude:

```
*.onnx
*.bin
*.safetensors
```

Only documentation and configuration files should be committed.

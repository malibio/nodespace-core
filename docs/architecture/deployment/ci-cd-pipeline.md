# CI/CD Pipeline Documentation

## Overview

NodeSpace uses GitHub Actions for continuous integration and deployment. The pipeline builds desktop applications for macOS and Windows.

## Workflows

### Release Workflow (`.github/workflows/release.yml`)

**Triggers:**
- `release`: When a GitHub release is created (production builds)
- `workflow_dispatch`: Manual trigger for testing (artifacts uploaded, not attached to release)

**Platforms:**
- macOS ARM (Apple Silicon - M1/M2/M3): `aarch64-apple-darwin`
- Windows: `windows-latest`

> **Note:** Windows builds may be temporarily disabled during testing. Check the workflow file for current status.

### Build Steps

1. **Checkout repository**
2. **Install Rust stable** with target architecture
3. **Rust cache** for faster builds
4. **Install Protobuf** (platform-specific)
5. **Setup Bun** (v1.1.38)
6. **Install frontend dependencies** (`bun install`)
7. **Download NLP models** (from GitHub release asset)
8. **Build Tauri app** (compiles Rust + bundles frontend)
9. **Upload artifacts** (for workflow_dispatch) or attach to release

## NLP Model Bundling

### Critical: Model Asset Dependency

The build pipeline downloads the NLP embedding model from a **GitHub release asset**:

- **Release:** `models-v1` ([link](https://github.com/malibio/nodespace-core/releases/tag/models-v1))
- **Asset:** `BAAI-bge-small-en-v1.5.tar.gz` (75MB)
- **Contains:** BAAI/bge-small-en-v1.5 sentence embedding model (safetensors format)

**WARNING:** Do NOT delete the `models-v1` release or its assets. This will break all CI/CD builds.

### Why GitHub Release Assets?

We initially tried downloading models directly from Hugging Face during CI/CD, but encountered persistent issues with Python environment configuration in GitHub Actions (multiple Python installations, PATH conflicts). Using a pre-packaged GitHub release asset is:

- **Reliable:** No external dependencies or Python configuration
- **Fast:** Direct download vs. Hugging Face API calls
- **Controlled:** We control the exact model version and contents

### Updating the Model

To update the bundled model:

1. Download the new model locally:
   ```bash
   bun run download:models  # Downloads to ~/.nodespace/models/
   ```

2. Create a tarball (excluding unnecessary files):
   ```bash
   cd ~/.nodespace/models
   tar --exclude='BAAI-bge-small-en-v1.5/.cache' \
       --exclude='BAAI-bge-small-en-v1.5/onnx' \
       -czvf /tmp/BAAI-bge-small-en-v1.5.tar.gz BAAI-bge-small-en-v1.5
   ```

3. Create a new release:
   ```bash
   gh release create models-v2 /tmp/BAAI-bge-small-en-v1.5.tar.gz \
     --title "NLP Models v2" \
     --notes "Updated NLP models for NodeSpace"
   ```

4. Update `.github/workflows/release.yml` to use `models-v2`:
   ```yaml
   gh release download models-v2 --pattern "BAAI-bge-small-en-v1.5.tar.gz"
   ```

5. Keep the old `models-v1` release until all in-flight builds complete

## Model Location in Built App

After bundling, the model is located at:

| Platform | Location |
|----------|----------|
| macOS | `NodeSpace.app/Contents/Resources/resources/models/BAAI-bge-small-en-v1.5/` |
| Windows | `resources/models/BAAI-bge-small-en-v1.5/` (next to .exe) |

The Rust backend resolves this path using Tauri's `BaseDirectory::Resource`.

## Testing the Pipeline

### Manual Test (No Release)

```bash
gh workflow run release.yml
```

This builds artifacts and uploads them as workflow artifacts (not attached to a release). Useful for testing changes.

### View Build Logs

```bash
gh run list --workflow=release.yml --limit 5
gh run view <run-id> --log
```

### Download Test Artifacts

After a successful `workflow_dispatch` run:
1. Go to the workflow run in GitHub Actions
2. Download artifacts from the "Artifacts" section

## Creating a Production Release

1. Ensure all tests pass and code is ready
2. Create a git tag:
   ```bash
   git tag v0.1.0-alpha.4
   git push origin v0.1.0-alpha.4
   ```
3. Create a GitHub release from the tag (or use `gh release create`)
4. The release workflow will automatically build and attach binaries

## Troubleshooting

### Build fails at "Download NLP models"
- Check that `models-v1` release exists and has the correct asset
- Verify `GH_TOKEN` is set (should be automatic with `GITHUB_TOKEN`)

### Rust compilation fails
- Check for Cargo.lock conflicts
- Try clearing the Rust cache (delete the workflow and re-run)

### Tauri bundling fails
- Ensure `tauri.conf.json` is valid
- Check that resources paths are correct

## Future Improvements

- [ ] Add macOS Intel (x86_64) support
- [ ] Add Linux (Ubuntu) support
- [ ] Code signing for macOS notarization
- [ ] Windows code signing
- [ ] Auto-update mechanism

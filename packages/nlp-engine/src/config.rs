/// Configuration for the embedding service using llama.cpp
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Offload all model layers to GPU. This is the llama.cpp convention
/// where any value >= total layers offloads everything.
pub const GPU_OFFLOAD_ALL_LAYERS: u32 = 99;

/// SHA-256 of the pinned embedding model `nomic-embed-text-v1.5.Q8_0.gguf`
/// (HuggingFace `nomic-ai/nomic-embed-text-v1.5-GGUF`, commit
/// `0188c9bf409793f810680a5a431e7b899c46104c`), lowercase hex.
///
/// This is the load-time half of the model-integrity gate (ADR-058); the
/// build-time download in `scripts/download-models.ts` pins the SAME digest.
/// Rotating the model MUST update both constants in the same change.
pub const EMBEDDING_MODEL_SHA256: &str =
    "3e24342164b3d94991ba9692fdc0dd08e3fd7362e0aacc396a9a5c54a544c3b7";

/// Compute the SHA-256 of the file at `path`, streaming in 64 KB chunks so a
/// ~146 MB GGUF does not balloon memory. Returns the lowercase-hex digest.
pub fn compute_file_sha256(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Verify the on-disk model at `path` against the pinned [`EMBEDDING_MODEL_SHA256`].
///
/// Returns `Err` describing the mismatch (or read failure) so the caller can
/// refuse to load a tampered or corrupt artifact. This closes the post-install
/// tamper window: even a correctly-downloaded model that is later swapped on
/// disk is rejected at load time.
pub fn verify_model_integrity(path: &Path) -> Result<(), String> {
    let actual = compute_file_sha256(path).map_err(|e| {
        format!(
            "failed to read model for integrity check {}: {}",
            path.display(),
            e
        )
    })?;
    if actual != EMBEDDING_MODEL_SHA256 {
        return Err(format!(
            "model integrity check FAILED for {}: expected SHA-256 {}, got {}",
            path.display(),
            EMBEDDING_MODEL_SHA256,
            actual
        ));
    }
    Ok(())
}

/// Configuration for llama.cpp embedding model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Model name or identifier
    pub model_name: String,

    /// Local model path (GGUF file)
    pub model_path: Option<PathBuf>,

    /// Number of GPU layers to offload. Use `GPU_OFFLOAD_ALL_LAYERS` (99) to offload all.
    pub n_gpu_layers: u32,

    /// Context size for embedding
    pub context_size: u32,

    /// Number of threads for CPU inference
    pub n_threads: i32,

    /// Maximum cache size (number of embeddings to cache)
    pub cache_capacity: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "nomic-embed-text-v1.5".to_string(),
            model_path: None,
            n_gpu_layers: GPU_OFFLOAD_ALL_LAYERS,
            context_size: 8192,
            n_threads: std::thread::available_parallelism()
                .map(|p| p.get() as i32)
                .unwrap_or(4),
            cache_capacity: 10000,
        }
    }
}

impl EmbeddingConfig {
    /// Get the model path, resolving it from ~/.nodespace/models/
    ///
    /// Uses centralized data directory pattern:
    /// - macOS/Linux: ~/.nodespace/models/nomic-embed-text-v1.5.Q8_0.gguf
    /// - Windows: %USERPROFILE%\.nodespace\models\nomic-embed-text-v1.5.Q8_0.gguf
    ///
    /// Resolution matches on filename only; the caller then enforces
    /// [`verify_model_integrity`] against [`EMBEDDING_MODEL_SHA256`] before
    /// loading. Only the pinned Q8_0 *bytes* pass that gate, so a different
    /// quantization (e.g. the `.f16` fallback) or a custom `model_path` pointing
    /// at some other model resolves here but is then rejected at load time —
    /// this is deliberate, so nothing but the pinned artifact reaches llama.cpp.
    pub fn resolve_model_path(&self) -> Result<PathBuf, std::io::Error> {
        if let Some(path) = &self.model_path {
            if path.exists() {
                return Ok(path.clone());
            }
        }

        // Use centralized ~/.nodespace/models/ directory
        let home_dir = dirs::home_dir().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Cannot determine home directory",
            )
        })?;

        // Try multiple possible filenames
        let base_path = home_dir.join(".nodespace").join("models");
        let possible_names = [
            format!("{}.gguf", sanitize_model_name(&self.model_name)),
            format!("{}.Q8_0.gguf", sanitize_model_name(&self.model_name)),
            format!("{}.f16.gguf", sanitize_model_name(&self.model_name)),
        ];

        for name in &possible_names {
            let model_path = base_path.join(name);
            if model_path.exists() {
                return Ok(model_path);
            }
        }

        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Model not found. Please download nomic-embed-text GGUF to ~/.nodespace/models/. Tried: {:?}",
                possible_names
            ),
        ))
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.model_name.is_empty() {
            return Err("model_name cannot be empty".to_string());
        }

        if self.context_size == 0 {
            return Err("context_size must be greater than 0".to_string());
        }

        if self.cache_capacity == 0 {
            return Err("cache_capacity must be greater than 0".to_string());
        }

        Ok(())
    }
}

/// Sanitize model name to be filesystem-safe
fn sanitize_model_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '<' | '>' | '|' | '"' => '-',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.model_name, "nomic-embed-text-v1.5");
        assert_eq!(config.n_gpu_layers, GPU_OFFLOAD_ALL_LAYERS);
        assert_eq!(config.context_size, 8192);
        assert_eq!(config.cache_capacity, 10000);
    }

    #[test]
    fn test_config_validation() {
        let mut config = EmbeddingConfig::default();

        // Valid config
        assert!(config.validate().is_ok());

        // Invalid: empty model name
        config.model_name = String::new();
        assert!(config.validate().is_err());

        // Invalid: zero context size
        config.model_name = "test".to_string();
        config.context_size = 0;
        assert!(config.validate().is_err());

        // Invalid: zero cache capacity
        config.context_size = 8192;
        config.cache_capacity = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_sanitize_model_name() {
        assert_eq!(sanitize_model_name("nomic/embed"), "nomic-embed");
        assert_eq!(sanitize_model_name("model:v1"), "model-v1");
        assert_eq!(sanitize_model_name("normal-name"), "normal-name");
    }

    #[test]
    fn test_pinned_digest_is_lowercase_hex_64() {
        assert_eq!(EMBEDDING_MODEL_SHA256.len(), 64);
        assert!(EMBEDDING_MODEL_SHA256
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn test_compute_file_sha256_known_vector() {
        use std::io::Write;
        // NIST SHA-256 test vector: sha256("abc").
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"abc").unwrap();
        let digest = compute_file_sha256(f.path()).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn test_verify_model_integrity_rejects_mismatch() {
        use std::io::Write;
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(b"not the real model bytes").unwrap();
        let result = verify_model_integrity(f.path());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("integrity check FAILED"));
        assert!(msg.contains(EMBEDDING_MODEL_SHA256));
    }

    #[test]
    fn test_verify_model_integrity_read_failure_is_error() {
        let result = verify_model_integrity(Path::new("/nonexistent/model.gguf"));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to read model"));
    }
}

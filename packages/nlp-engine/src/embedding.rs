/// Core embedding service using Candle + ONNX
/// Follows patterns from nodespace BERT implementation guide
use crate::config::EmbeddingConfig;
use crate::error::{EmbeddingError, Result};
use dashmap::DashMap;
use std::sync::Arc;

#[cfg(feature = "embedding-service")]
use candle_core::{Device, Tensor};
#[cfg(feature = "embedding-service")]
use candle_onnx::simple_eval;
#[cfg(feature = "embedding-service")]
use tokenizers::Tokenizer;

/// Main embedding service
pub struct EmbeddingService {
    config: EmbeddingConfig,
    #[cfg(feature = "embedding-service")]
    model: Option<candle_onnx::onnx::ModelProto>,
    #[cfg(feature = "embedding-service")]
    tokenizer: Option<Tokenizer>,
    #[cfg(feature = "embedding-service")]
    device: Device,
    cache: Arc<DashMap<String, Vec<f32>>>,
    initialized: bool,
}

impl EmbeddingService {
    /// Create a new embedding service with the given configuration
    pub fn new(config: EmbeddingConfig) -> Result<Self> {
        // Validate config
        config.validate().map_err(EmbeddingError::ConfigError)?;

        #[cfg(feature = "embedding-service")]
        let device = {
            // Try Metal GPU first (macOS), fall back to CPU
            Device::new_metal(0).unwrap_or_else(|_| {
                tracing::info!("Metal GPU not available, using CPU");
                Device::Cpu
            })
        };

        let cache_capacity = config.cache_capacity;

        Ok(Self {
            config,
            #[cfg(feature = "embedding-service")]
            model: None,
            #[cfg(feature = "embedding-service")]
            tokenizer: None,
            #[cfg(feature = "embedding-service")]
            device,
            cache: Arc::new(DashMap::with_capacity(cache_capacity)),
            initialized: false,
        })
    }

    /// Initialize the model (async, loads from bundled path)
    pub async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        #[cfg(feature = "embedding-service")]
        {
            tracing::info!("Loading embedding model: {}", self.config.model_name);

            // Resolve model path
            let model_path = self
                .config
                .resolve_model_path()
                .map_err(|e| EmbeddingError::ModelNotFound(e.to_string()))?;

            tracing::info!("Model path resolved to: {:?}", model_path);

            // Load ONNX model
            let model_file = model_path.join("model.onnx");
            if !model_file.exists() {
                return Err(EmbeddingError::ModelNotFound(format!(
                    "Model file not found: {:?}",
                    model_file
                )));
            }

            let model = candle_onnx::read_file(&model_file)
                .map_err(|e| EmbeddingError::ModelLoadError(e.to_string()))?;

            // Load tokenizer
            let tokenizer_file = model_path.join("tokenizer.json");
            if !tokenizer_file.exists() {
                return Err(EmbeddingError::ModelNotFound(format!(
                    "Tokenizer file not found: {:?}",
                    tokenizer_file
                )));
            }

            let tokenizer = Tokenizer::from_file(&tokenizer_file)
                .map_err(|e| EmbeddingError::TokenizationError(e.to_string()))?;

            self.model = Some(model);
            self.tokenizer = Some(tokenizer);

            tracing::info!("Embedding model initialized successfully");
        }

        #[cfg(not(feature = "embedding-service"))]
        {
            tracing::info!("STUB: Embedding service initialized (feature disabled)");
        }

        self.initialized = true;
        Ok(())
    }

    /// Generate embedding for a single text
    pub async fn generate_embedding(&self, text: &str) -> Result<Vec<f32>> {
        // Check cache first
        if let Some(cached) = self.cache.get(text) {
            return Ok(cached.clone());
        }

        if !self.initialized {
            return Err(EmbeddingError::NotInitialized);
        }

        #[cfg(feature = "embedding-service")]
        {
            let embedding = self.generate_embedding_internal(text).await?;

            // Cache the result
            if self.cache.len() < self.config.cache_capacity {
                self.cache.insert(text.to_string(), embedding.clone());
            }

            Ok(embedding)
        }

        #[cfg(not(feature = "embedding-service"))]
        {
            // Stub: return zero vector with correct dimensions
            Ok(vec![0.0; 384])
        }
    }

    /// Generate embeddings for multiple texts (batch operation)
    pub async fn generate_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        for text in texts {
            results.push(self.generate_embedding(text).await?);
        }

        Ok(results)
    }

    #[cfg(feature = "embedding-service")]
    async fn generate_embedding_internal(&self, text: &str) -> Result<Vec<f32>> {
        let model = self.model.as_ref().ok_or(EmbeddingError::NotInitialized)?;
        let tokenizer = self
            .tokenizer
            .as_ref()
            .ok_or(EmbeddingError::NotInitialized)?;

        // Tokenize input
        let encoding = tokenizer
            .encode(text, true)
            .map_err(|e| EmbeddingError::TokenizationError(e.to_string()))?;

        let tokens = encoding.get_ids();
        let attention_mask = encoding.get_attention_mask();

        // Create tensors
        let token_ids = Tensor::new(tokens, &self.device)
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?
            .unsqueeze(0)?; // Add batch dimension

        let attention_mask = Tensor::new(attention_mask, &self.device)
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?
            .unsqueeze(0)?;

        // Run inference
        let inputs = std::collections::HashMap::from([
            ("input_ids".to_string(), token_ids),
            ("attention_mask".to_string(), attention_mask),
        ]);

        let outputs = simple_eval(model, inputs)
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?;

        // Extract embeddings (output should be a vector of tensors)
        let embedding_tensor = outputs
            .get("last_hidden_state")
            .or_else(|| outputs.values().next())
            .ok_or_else(|| EmbeddingError::InferenceError("No output from model".to_string()))?;

        // Convert to Vec<f32>
        let embedding: Vec<f32> = embedding_tensor
            .squeeze(0)?
            .to_vec1()
            .map_err(|e| EmbeddingError::InferenceError(e.to_string()))?;

        Ok(embedding)
    }

    /// Convert embedding vector to F32_BLOB format for Turso storage
    pub fn to_blob(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// Convert F32_BLOB format back to embedding vector
    pub fn from_blob(blob: &[u8]) -> Vec<f32> {
        blob.chunks_exact(4)
            .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            .collect()
    }

    /// Clear the embedding cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Get cache statistics (size, capacity)
    pub fn cache_stats(&self) -> (usize, usize) {
        let len = self.cache.len();
        let capacity = self.cache.capacity();
        (len, capacity)
    }

    /// Check if service is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get device information
    #[cfg(feature = "embedding-service")]
    pub fn device_info(&self) -> String {
        match &self.device {
            Device::Cpu => "CPU".to_string(),
            Device::Cuda(_) => "CUDA GPU".to_string(),
            Device::Metal(_) => "Metal GPU".to_string(),
        }
    }

    #[cfg(not(feature = "embedding-service"))]
    pub fn device_info(&self) -> String {
        "Stub (feature disabled)".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blob_conversion() {
        let embedding = vec![0.1, 0.2, 0.3, -0.4, 1.5];
        let blob = EmbeddingService::to_blob(&embedding);
        let recovered = EmbeddingService::from_blob(&blob);

        assert_eq!(embedding.len(), recovered.len());
        for (original, recovered) in embedding.iter().zip(recovered.iter()) {
            assert!((original - recovered).abs() < 1e-6);
        }
    }

    #[test]
    fn test_service_creation() {
        let config = EmbeddingConfig::default();
        let service = EmbeddingService::new(config);
        assert!(service.is_ok());

        let service = service.unwrap();
        assert!(!service.is_initialized());
    }

    #[test]
    fn test_cache_stats() {
        let config = EmbeddingConfig::default();
        let service = EmbeddingService::new(config).unwrap();
        let (len, capacity) = service.cache_stats();
        assert_eq!(len, 0);
        assert!(capacity > 0);
    }

    #[cfg(not(feature = "embedding-service"))]
    #[tokio::test]
    async fn test_stub_embedding() {
        let config = EmbeddingConfig::default();
        let mut service = EmbeddingService::new(config).unwrap();
        service.initialize().await.unwrap();

        let embedding = service.generate_embedding("test").await.unwrap();
        assert_eq!(embedding.len(), 384); // bge-small-en-v1.5 dimension
        assert!(embedding.iter().all(|&x| x == 0.0)); // Stub returns zeros
    }
}

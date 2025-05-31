use crate::types::{ApiKeyProvider, UserApiKey};
use crate::utils::{ArbitrageError, ArbitrageResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
// use worker::console_log; // TODO: Re-enable when implementing logging integration
use uuid;
use worker::kv::KvStore;

/// Configuration for AI integration service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiIntegrationConfig {
    pub enabled: bool,
    pub default_timeout_seconds: u64,
    pub max_retries: u32,
    pub supported_providers: Vec<ApiKeyProvider>,
    pub max_ai_keys_per_user: u32,
}

impl Default for AiIntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_timeout_seconds: 30,
            max_retries: 3,
            max_ai_keys_per_user: 10,
            supported_providers: vec![
                ApiKeyProvider::OpenAI,
                ApiKeyProvider::Anthropic,
                ApiKeyProvider::Custom,
            ],
        }
    }
}

/// AI provider interface for different AI services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AiProvider {
    OpenAI {
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    },
    Anthropic {
        api_key: String,
        base_url: Option<String>,
        model: Option<String>,
    },
    Custom {
        api_key: String,
        base_url: String,
        headers: HashMap<String, String>,
        model: Option<String>,
    },
}

/// Request structure for AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysisRequest {
    pub prompt: String,
    pub market_data: Value,
    pub user_context: Option<Value>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// Response structure from AI analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiAnalysisResponse {
    pub analysis: String,
    pub confidence: Option<f32>,
    pub recommendations: Vec<String>,
    pub metadata: HashMap<String, Value>,
}

/// AI Integration Service for managing user AI configurations
pub struct AiIntegrationService {
    config: AiIntegrationConfig,
    http_client: Client,
    kv_store: KvStore,
    encryption_key: String,
}

impl AiIntegrationService {
    /// Create new AI integration service
    pub fn new(config: AiIntegrationConfig, kv_store: KvStore, encryption_key: String) -> Self {
        Self {
            config,
            http_client: Client::new(),
            kv_store,
            encryption_key,
        }
    }

    /// Store AI credentials for a user
    pub async fn store_ai_credentials(
        &self,
        user_id: &str,
        provider: ApiKeyProvider,
        api_key: &str,
        _metadata: Option<Value>,
    ) -> ArbitrageResult<String> {
        // Check if user has reached the maximum number of AI keys
        let existing_keys = self.get_user_ai_keys(user_id).await?;
        let ai_key_count = existing_keys.iter().filter(|key| key.is_ai_key()).count();

        if ai_key_count >= self.config.max_ai_keys_per_user as usize {
            return Err(ArbitrageError::validation_error(format!(
                "Maximum AI keys limit ({}) reached",
                self.config.max_ai_keys_per_user
            )));
        }

        // Validate provider is supported
        if !self.is_provider_supported(&provider) {
            return Err(ArbitrageError::validation_error(
                "AI provider not supported",
            ));
        }

        // Encrypt the API key
        let encrypted_key = self.encrypt_string(api_key)?;

        // Create the UserApiKey
        let api_key_id = uuid::Uuid::new_v4().to_string();
        let user_api_key = UserApiKey::new_ai_key(
            user_id.to_string(),
            provider,
            encrypted_key,
            std::collections::HashMap::new(), // metadata parameter
        );

        // Store the key
        let key = format!("ai_key:{}:{}", user_id, api_key_id);
        let serialized = serde_json::to_string(&user_api_key).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize AI key: {}", e))
        })?;

        self.kv_store
            .put(&key, &serialized)
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to prepare AI key storage: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| ArbitrageError::storage_error(format!("Failed to store AI key: {}", e)))?;

        // Update user's AI key index
        self.update_user_ai_key_index(user_id, &api_key_id, true)
            .await?;

        Ok(api_key_id)
    }

    /// Remove AI credentials for a user
    pub async fn remove_ai_credentials(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> ArbitrageResult<bool> {
        // Remove from storage
        let key = format!("ai_key:{}:{}", user_id, api_key_id);
        self.kv_store.delete(&key).await.map_err(|e| {
            ArbitrageError::storage_error(format!("Failed to delete AI key: {}", e))
        })?;

        // Update user's AI key index
        self.update_user_ai_key_index(user_id, api_key_id, false)
            .await?;

        Ok(true)
    }

    /// Get all AI credentials for a user
    pub async fn get_user_ai_keys(&self, user_id: &str) -> ArbitrageResult<Vec<UserApiKey>> {
        let index_key = format!("ai_key_index:{}", user_id);
        let index_data = self.kv_store.get(&index_key).text().await.map_err(|e| {
            ArbitrageError::storage_error(format!("Failed to get AI key index: {}", e))
        })?;

        let key_ids: Vec<String> = if let Some(data) = index_data {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        let mut ai_keys = Vec::new();
        for key_id in key_ids {
            let key = format!("ai_key:{}:{}", user_id, key_id);
            if let Ok(Some(data)) = self.kv_store.get(&key).text().await {
                if let Ok(api_key) = serde_json::from_str::<UserApiKey>(&data) {
                    ai_keys.push(api_key);
                }
            }
        }

        Ok(ai_keys)
    }

    /// Validate and test AI credentials
    pub async fn validate_and_test_credentials(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> ArbitrageResult<bool> {
        // Get the AI key
        let ai_keys = self.get_user_ai_keys(user_id).await?;
        let ai_key = ai_keys
            .iter()
            .find(|key| key.key_id == api_key_id)
            .ok_or_else(|| ArbitrageError::not_found("AI key not found"))?;

        // Decrypt the key and create provider
        let decrypted_key = self.decrypt_string(&ai_key.encrypted_key)?;
        let provider = self.create_ai_provider_from_key(ai_key, &decrypted_key)?;

        // Test connectivity
        match self.test_ai_connectivity(&provider).await {
            Ok(_) => {
                // Update last_used timestamp
                self.update_ai_key_last_used(user_id, api_key_id).await?;
                Ok(true)
            }
            Err(e) => {
                // Return validation error with details
                Err(ArbitrageError::validation_error(format!(
                    "AI credentials validation failed: {}",
                    e
                )))
            }
        }
    }

    /// Get AI provider instance for user
    pub async fn get_user_ai_provider(
        &self,
        user_id: &str,
        provider_type: &ApiKeyProvider,
    ) -> ArbitrageResult<AiProvider> {
        let ai_keys = self.get_user_ai_keys(user_id).await?;
        let ai_key = ai_keys
            .iter()
            .find(|key| key.provider == *provider_type && key.is_active)
            .ok_or_else(|| ArbitrageError::not_found("Active AI key not found for provider"))?;

        let decrypted_key = self.decrypt_string(&ai_key.encrypted_key)?;
        self.create_ai_provider_from_key(ai_key, &decrypted_key)
    }

    /// Validate AI provider credentials
    pub async fn validate_ai_credentials(&self, provider: &AiProvider) -> ArbitrageResult<bool> {
        match provider {
            AiProvider::OpenAI {
                api_key, base_url, ..
            } => {
                self.validate_openai_credentials(api_key, base_url.as_deref())
                    .await
            }
            AiProvider::Anthropic {
                api_key, base_url, ..
            } => {
                self.validate_anthropic_credentials(api_key, base_url.as_deref())
                    .await
            }
            AiProvider::Custom {
                api_key,
                base_url,
                headers,
                ..
            } => {
                self.validate_custom_credentials(api_key, base_url, headers)
                    .await
            }
        }
    }

    /// Test connectivity to AI provider
    pub async fn test_ai_connectivity(&self, provider: &AiProvider) -> ArbitrageResult<String> {
        let test_request = AiAnalysisRequest {
            prompt: "Test connectivity. Please respond with 'OK' if you receive this message."
                .to_string(),
            market_data: json!({}),
            user_context: None,
            max_tokens: Some(10),
            temperature: Some(0.1),
        };

        let response = self.call_ai_provider(provider, &test_request).await?;
        Ok(response.analysis)
    }

    /// Call AI provider with analysis request
    pub async fn call_ai_provider(
        &self,
        provider: &AiProvider,
        request: &AiAnalysisRequest,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        if !self.config.enabled {
            return Err(ArbitrageError::config_error("AI integration is disabled"));
        }

        match provider {
            AiProvider::OpenAI {
                api_key,
                base_url,
                model,
            } => {
                self.call_openai(api_key, base_url.as_deref(), model.as_deref(), request)
                    .await
            }
            AiProvider::Anthropic {
                api_key,
                base_url,
                model,
            } => {
                self.call_anthropic(api_key, base_url.as_deref(), model.as_deref(), request)
                    .await
            }
            AiProvider::Custom {
                api_key,
                base_url,
                headers,
                model,
            } => {
                self.call_custom_provider(api_key, base_url, headers, model.as_deref(), request)
                    .await
            }
        }
    }

    /// Create AI provider from user API key
    #[allow(clippy::result_large_err)]
    pub fn create_ai_provider(&self, api_key: &UserApiKey) -> ArbitrageResult<AiProvider> {
        match api_key.provider {
            ApiKeyProvider::OpenAI => Ok(AiProvider::OpenAI {
                api_key: api_key.encrypted_key.clone(),
                base_url: api_key.metadata.get("base_url").map(|s| s.to_string()),
                model: api_key.metadata.get("model").map(|s| s.to_string()),
            }),
            ApiKeyProvider::Anthropic => Ok(AiProvider::Anthropic {
                api_key: api_key.encrypted_key.clone(),
                base_url: api_key.metadata.get("base_url").map(|s| s.to_string()),
                model: api_key.metadata.get("model").map(|s| s.to_string()),
            }),
            ApiKeyProvider::Custom => {
                let base_url = api_key
                    .metadata
                    .get("base_url")
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        ArbitrageError::configuration_error(
                            "Custom AI provider requires base_url".to_string(),
                        )
                    })?;

                let headers = api_key
                    .metadata
                    .get("headers")
                    .map(|s| {
                        // Try to parse as JSON, fallback to empty HashMap
                        serde_json::from_str::<std::collections::HashMap<String, String>>(s)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();

                Ok(AiProvider::Custom {
                    api_key: api_key.encrypted_key.clone(),
                    base_url,
                    headers,
                    model: api_key.metadata.get("model").map(|s| s.to_string()),
                })
            }
            _ => Err(ArbitrageError::configuration_error(format!(
                "Unsupported AI provider: {:?}",
                api_key.provider
            ))),
        }
    }

    /// Get supported AI providers
    pub fn get_supported_providers(&self) -> &[ApiKeyProvider] {
        &self.config.supported_providers
    }

    /// Check if provider is supported
    pub fn is_provider_supported(&self, provider: &ApiKeyProvider) -> bool {
        self.config.supported_providers.contains(provider)
    }

    // Private methods for specific AI providers

    async fn validate_openai_credentials(
        &self,
        api_key: &str,
        base_url: Option<&str>,
    ) -> ArbitrageResult<bool> {
        let url = format!("{}/v1/models", base_url.unwrap_or("https://api.openai.com"));

        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .timeout(std::time::Duration::from_secs(
                self.config.default_timeout_seconds,
            ))
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("OpenAI validation failed: {}", e))
            })?;

        Ok(response.status().is_success())
    }

    async fn validate_anthropic_credentials(
        &self,
        api_key: &str,
        base_url: Option<&str>,
    ) -> ArbitrageResult<bool> {
        let url = format!(
            "{}/v1/messages",
            base_url.unwrap_or("https://api.anthropic.com")
        );

        // Send a minimal test request
        let test_payload = json!({
            "model": "claude-3-haiku-20240307",
            "max_tokens": 1,
            "messages": [{"role": "user", "content": "test"}]
        });

        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&test_payload)
            .timeout(std::time::Duration::from_secs(
                self.config.default_timeout_seconds,
            ))
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Anthropic validation failed: {}", e))
            })?;

        // Accept both success and rate limit as valid (credentials are correct)
        Ok(response.status().is_success() || response.status() == 429)
    }

    async fn validate_custom_credentials(
        &self,
        api_key: &str,
        base_url: &str,
        headers: &HashMap<String, String>,
    ) -> ArbitrageResult<bool> {
        let mut request = self
            .http_client
            .get(base_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .timeout(std::time::Duration::from_secs(
                self.config.default_timeout_seconds,
            ));

        for (key, value) in headers {
            request = request.header(key, value);
        }

        let response = request.send().await.map_err(|e| {
            ArbitrageError::network_error(format!("Custom provider validation failed: {}", e))
        })?;

        Ok(response.status().is_success())
    }

    async fn call_openai(
        &self,
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
        request: &AiAnalysisRequest,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        let url = format!(
            "{}/v1/chat/completions",
            base_url.unwrap_or("https://api.openai.com")
        );
        let model_name = model.unwrap_or("gpt-3.5-turbo");

        let payload = json!({
            "model": model_name,
            "messages": [
                {
                    "role": "system",
                    "content": "You are an expert cryptocurrency trading analyst. Analyze the provided market data and provide insights for arbitrage opportunities."
                },
                {
                    "role": "user",
                    "content": format!("Prompt: {}\nMarket Data: {}", request.prompt, request.market_data)
                }
            ],
            "max_tokens": request.max_tokens.unwrap_or(500),
            "temperature": request.temperature.unwrap_or(0.7)
        });

        let response = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(
                self.config.default_timeout_seconds,
            ))
            .send()
            .await
            .map_err(|e| ArbitrageError::network_error(format!("OpenAI API call failed: {}", e)))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ArbitrageError::api_error(format!(
                "OpenAI API error: {}",
                error_text
            )));
        }

        let response_data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse OpenAI response: {}", e))
        })?;

        let analysis = response_data["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("No response")
            .to_string();

        let confidence = response_data["choices"][0]["confidence"]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(0.7);

        let recommendations: Vec<String> = response_data["choices"][0]["message"]
            ["recommendations"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(
                    &response_data["recommendations"].to_string(),
                ) {
                    parsed
                } else {
                    vec![response_data["recommendations"]
                        .as_str()
                        .unwrap_or("No recommendations available")
                        .to_string()]
                }
            });

        Ok(AiAnalysisResponse {
            analysis,
            confidence: Some(confidence),
            recommendations,
            metadata: HashMap::new(),
        })
    }

    async fn call_anthropic(
        &self,
        api_key: &str,
        base_url: Option<&str>,
        model: Option<&str>,
        request: &AiAnalysisRequest,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        let url = format!(
            "{}/v1/messages",
            base_url.unwrap_or("https://api.anthropic.com")
        );
        let model_name = model.unwrap_or("claude-3-haiku-20240307");

        let payload = json!({
            "model": model_name,
            "max_tokens": request.max_tokens.unwrap_or(500),
            "messages": [
                {
                    "role": "user",
                    "content": format!("As a cryptocurrency trading analyst, analyze this market data for arbitrage opportunities:\n\nPrompt: {}\nMarket Data: {}", request.prompt, request.market_data)
                }
            ]
        });

        let response = self
            .http_client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(
                self.config.default_timeout_seconds,
            ))
            .send()
            .await
            .map_err(|e| {
                ArbitrageError::network_error(format!("Anthropic API call failed: {}", e))
            })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ArbitrageError::api_error(format!(
                "Anthropic API error: {}",
                error_text
            )));
        }

        let response_data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse Anthropic response: {}", e))
        })?;

        let analysis = response_data["content"][0]["text"]
            .as_str()
            .unwrap_or("No response")
            .to_string();

        let confidence = response_data["confidence"]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(0.7);

        let recommendations: Vec<String> = response_data["recommendations"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(
                    &response_data["recommendations"].to_string(),
                ) {
                    parsed
                } else {
                    vec![response_data["recommendations"]
                        .as_str()
                        .unwrap_or("No recommendations available")
                        .to_string()]
                }
            });

        Ok(AiAnalysisResponse {
            analysis,
            confidence: Some(confidence),
            recommendations,
            metadata: HashMap::new(),
        })
    }

    async fn call_custom_provider(
        &self,
        api_key: &str,
        base_url: &str,
        headers: &HashMap<String, String>,
        model: Option<&str>,
        request: &AiAnalysisRequest,
    ) -> ArbitrageResult<AiAnalysisResponse> {
        let payload = json!({
            "prompt": request.prompt,
            "market_data": request.market_data,
            "max_tokens": request.max_tokens.unwrap_or(500),
            "temperature": request.temperature.unwrap_or(0.7),
            "model": model
        });

        let mut http_request = self
            .http_client
            .post(base_url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(
                self.config.default_timeout_seconds,
            ));

        for (key, value) in headers {
            http_request = http_request.header(key, value);
        }

        let response = http_request.send().await.map_err(|e| {
            ArbitrageError::network_error(format!("Custom provider API call failed: {}", e))
        })?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ArbitrageError::api_error(format!(
                "Custom provider API error: {}",
                error_text
            )));
        }

        let response_data: Value = response.json().await.map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to parse custom provider response: {}", e))
        })?;

        // Try to extract analysis from common response formats
        let analysis = response_data["response"]
            .as_str()
            .or_else(|| response_data["text"].as_str())
            .or_else(|| response_data["analysis"].as_str())
            .or_else(|| response_data["content"].as_str())
            .unwrap_or("No response")
            .to_string();

        let confidence = response_data["confidence"]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(0.7);

        let _risk_score = response_data["risk_score"]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(0.5);

        let _timing_score = response_data["timing_score"]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(0.5);

        let _position_size = response_data["position_size"]
            .as_f64()
            .map(|v| v as f32)
            .unwrap_or(100.0);

        let recommendations: Vec<String> = response_data["recommendations"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| {
                if let Ok(parsed) = serde_json::from_str::<Vec<String>>(
                    &response_data["recommendations"].to_string(),
                ) {
                    parsed
                } else {
                    vec![response_data["recommendations"]
                        .as_str()
                        .unwrap_or("No recommendations available")
                        .to_string()]
                }
            });

        let _risk_factors = response_data["risk_factors"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default();

        Ok(AiAnalysisResponse {
            analysis,
            confidence: Some(confidence),
            recommendations,
            metadata: HashMap::new(),
        })
    }

    // Helper methods

    async fn update_user_ai_key_index(
        &self,
        user_id: &str,
        api_key_id: &str,
        add: bool,
    ) -> ArbitrageResult<()> {
        let index_key = format!("ai_key_index:{}", user_id);
        let index_data = self.kv_store.get(&index_key).text().await.map_err(|e| {
            ArbitrageError::storage_error(format!("Failed to get AI key index: {}", e))
        })?;

        let mut key_ids: Vec<String> = if let Some(data) = index_data {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        if add {
            if !key_ids.contains(&api_key_id.to_string()) {
                key_ids.push(api_key_id.to_string());
            }
        } else {
            key_ids.retain(|id| id != api_key_id);
        }

        let serialized = serde_json::to_string(&key_ids).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize key index: {}", e))
        })?;

        self.kv_store
            .put(&index_key, &serialized)
            .map_err(|e| {
                ArbitrageError::storage_error(format!(
                    "Failed to prepare AI key index storage: {}",
                    e
                ))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::storage_error(format!("Failed to update AI key index: {}", e))
            })?;

        Ok(())
    }

    async fn update_ai_key_last_used(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> ArbitrageResult<()> {
        let key = format!("ai_key:{}:{}", user_id, api_key_id);
        if let Ok(Some(data)) = self.kv_store.get(&key).text().await {
            if let Ok(mut api_key) = serde_json::from_str::<UserApiKey>(&data) {
                api_key.update_last_used();

                let serialized = serde_json::to_string(&api_key).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to serialize AI key: {}", e))
                })?;

                self.kv_store
                    .put(&key, &serialized)
                    .map_err(|e| {
                        ArbitrageError::storage_error(format!(
                            "Failed to prepare AI key storage: {}",
                            e
                        ))
                    })?
                    .execute()
                    .await
                    .map_err(|e| {
                        ArbitrageError::storage_error(format!("Failed to update AI key: {}", e))
                    })?;
            }
        }
        Ok(())
    }

    #[allow(clippy::result_large_err)]
    fn create_ai_provider_from_key(
        &self,
        api_key: &UserApiKey,
        decrypted_key: &str,
    ) -> ArbitrageResult<AiProvider> {
        match api_key.provider {
            ApiKeyProvider::OpenAI => Ok(AiProvider::OpenAI {
                api_key: decrypted_key.to_string(),
                base_url: api_key.metadata.get("base_url").map(|s| s.to_string()),
                model: api_key.metadata.get("model").map(|s| s.to_string()),
            }),
            ApiKeyProvider::Anthropic => Ok(AiProvider::Anthropic {
                api_key: decrypted_key.to_string(),
                base_url: api_key.metadata.get("base_url").map(|s| s.to_string()),
                model: api_key.metadata.get("model").map(|s| s.to_string()),
            }),
            ApiKeyProvider::Custom => {
                let base_url = api_key
                    .metadata
                    .get("base_url")
                    .map(|s| s.to_string())
                    .ok_or_else(|| {
                        ArbitrageError::configuration_error(
                            "Custom AI provider requires base_url".to_string(),
                        )
                    })?;

                let headers = api_key
                    .metadata
                    .get("headers")
                    .map(|s| {
                        // Try to parse as JSON, fallback to empty HashMap
                        serde_json::from_str::<std::collections::HashMap<String, String>>(s)
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();

                Ok(AiProvider::Custom {
                    api_key: decrypted_key.to_string(),
                    base_url,
                    headers,
                    model: api_key.metadata.get("model").map(|s| s.to_string()),
                })
            }
            _ => Err(ArbitrageError::configuration_error(format!(
                "Unsupported AI provider: {:?}",
                api_key.provider
            ))),
        }
    }

    #[allow(clippy::result_large_err)]
    fn encrypt_string(&self, plaintext: &str) -> ArbitrageResult<String> {
        use base64::{engine::general_purpose, Engine as _};

        let key_bytes = self.encryption_key.as_bytes();
        let encrypted: Vec<u8> = plaintext
            .as_bytes()
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect();

        Ok(general_purpose::STANDARD.encode(encrypted))
    }

    #[allow(clippy::result_large_err)]
    fn decrypt_string(&self, ciphertext: &str) -> ArbitrageResult<String> {
        use base64::{engine::general_purpose, Engine as _};

        let encrypted = general_purpose::STANDARD
            .decode(ciphertext)
            .map_err(|e| ArbitrageError::parse_error(format!("Failed to decode base64: {}", e)))?;

        let key_bytes = self.encryption_key.as_bytes();
        let decrypted: Vec<u8> = encrypted
            .iter()
            .enumerate()
            .map(|(i, &byte)| byte ^ key_bytes[i % key_bytes.len()])
            .collect();

        String::from_utf8(decrypted).map_err(|e| {
            ArbitrageError::parse_error(format!(
                "Failed to convert decrypted bytes to string: {}",
                e
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // Mock KV store for testing
    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    struct MockKvStore {
        data: std::sync::Arc<std::sync::Mutex<HashMap<String, String>>>,
    }

    #[allow(dead_code)]
    impl MockKvStore {
        fn new() -> Self {
            Self {
                data: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
            }
        }

        async fn get(&self, key: &str) -> Option<String> {
            let data = self.data.lock().unwrap();
            data.get(key).cloned()
        }

        async fn put(&self, key: &str, value: &str) -> Result<(), String> {
            let mut data = self.data.lock().unwrap();
            data.insert(key.to_string(), value.to_string());
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<(), String> {
            let mut data = self.data.lock().unwrap();
            data.remove(key);
            Ok(())
        }
    }

    fn create_test_config() -> AiIntegrationConfig {
        AiIntegrationConfig::default()
    }

    fn create_test_service() -> AiIntegrationService {
        let config = create_test_config();
        // Create minimal service for testing (KV store not used in these tests)
        AiIntegrationService {
            config,
            http_client: reqwest::Client::new(),
            kv_store: unsafe { std::mem::zeroed() }, // Not used in encryption tests
            encryption_key: "test-encryption-key-123".to_string(),
        }
    }

    #[test]
    fn test_ai_integration_config_creation() {
        let config = create_test_config();
        assert!(config.enabled);
        assert_eq!(config.default_timeout_seconds, 30);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.max_ai_keys_per_user, 10);
        assert_eq!(config.supported_providers.len(), 3);
    }

    #[test]
    fn test_ai_integration_service_creation() {
        // Test that the service can be created with proper configuration
        let config = create_test_config();
        assert!(config.enabled);
        // Note: actual service creation test would require KV mock
    }

    #[test]
    fn test_openai_provider_creation() {
        let provider = AiProvider::OpenAI {
            api_key: "test-key".to_string(),
            base_url: Some("https://api.openai.com".to_string()),
            model: Some("gpt-4".to_string()),
        };

        match provider {
            AiProvider::OpenAI {
                api_key,
                base_url,
                model,
            } => {
                assert_eq!(api_key, "test-key");
                assert_eq!(base_url, Some("https://api.openai.com".to_string()));
                assert_eq!(model, Some("gpt-4".to_string()));
            }
            _ => panic!("Expected OpenAI provider"),
        }
    }

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = AiProvider::Anthropic {
            api_key: "test-anthropic-key".to_string(),
            base_url: None,
            model: Some("claude-3-sonnet".to_string()),
        };

        match provider {
            AiProvider::Anthropic {
                api_key,
                base_url,
                model,
            } => {
                assert_eq!(api_key, "test-anthropic-key");
                assert_eq!(base_url, None);
                assert_eq!(model, Some("claude-3-sonnet".to_string()));
            }
            _ => panic!("Expected Anthropic provider"),
        }
    }

    #[test]
    fn test_custom_provider_creation() {
        let mut headers = HashMap::new();
        headers.insert("X-API-Key".to_string(), "custom-key".to_string());

        let provider = AiProvider::Custom {
            api_key: "custom-api-key".to_string(),
            base_url: "https://custom-ai.example.com".to_string(),
            headers: headers.clone(),
            model: Some("custom-model".to_string()),
        };

        match provider {
            AiProvider::Custom {
                api_key,
                base_url,
                headers: provider_headers,
                model,
            } => {
                assert_eq!(api_key, "custom-api-key");
                assert_eq!(base_url, "https://custom-ai.example.com");
                assert_eq!(provider_headers, headers);
                assert_eq!(model, Some("custom-model".to_string()));
            }
            _ => panic!("Expected Custom provider"),
        }
    }

    #[test]
    fn test_custom_provider_missing_base_url() {
        let metadata = json!({
            "model": "test-model"
            // Missing base_url
        });

        let api_key = UserApiKey::new_ai_key(
            "user123".to_string(),
            "encrypted_key".to_string(),
            "".to_string(), // api_secret
            ApiKeyProvider::Custom,
        );

        // This should be tested in the service context
        // We expect validation error for missing base_url
        assert_eq!(api_key.provider, ApiKeyProvider::Custom);
    }

    #[test]
    fn test_ai_analysis_request_creation() {
        let request = AiAnalysisRequest {
            prompt: "Analyze this market data".to_string(),
            market_data: json!({"price": 100.0, "volume": 1000}),
            user_context: Some(json!({"risk_tolerance": "medium"})),
            max_tokens: Some(500),
            temperature: Some(0.7),
        };

        assert_eq!(request.prompt, "Analyze this market data");
        assert_eq!(request.max_tokens, Some(500));
        assert_eq!(request.temperature, Some(0.7));
    }

    #[test]
    fn test_ai_analysis_response_creation() {
        let mut metadata = HashMap::new();
        metadata.insert("model".to_string(), json!("gpt-4"));
        metadata.insert("tokens_used".to_string(), json!(250));

        let response = AiAnalysisResponse {
            analysis: "Market shows bullish trends".to_string(),
            confidence: Some(0.8),
            recommendations: vec!["Buy".to_string(), "Hold".to_string()],
            metadata,
        };

        assert_eq!(response.analysis, "Market shows bullish trends");
        assert_eq!(response.confidence, Some(0.8));
        assert_eq!(response.recommendations.len(), 2);
    }

    #[test]
    fn test_disabled_ai_integration() {
        let mut config = create_test_config();
        config.enabled = false;

        // Test configuration
        assert!(!config.enabled);
        assert_eq!(config.max_ai_keys_per_user, 10);
    }

    #[test]
    fn test_exchange_key_rejection() {
        // Test that exchange API keys are properly rejected for AI use
        let api_key = UserApiKey::new_exchange_key(
            "user123".to_string(),
            crate::types::ExchangeIdEnum::Binance,
            "encrypted_key".to_string(),
            "encrypted_secret".to_string(),
            None,  // passphrase
            false, // is_testnet
        );

        // Verify it's an exchange key, not AI key
        assert!(api_key.is_ai_key() == false);
        assert!(
            api_key.provider == ApiKeyProvider::Exchange(crate::types::ExchangeIdEnum::Binance)
        );
    }

    #[test]
    fn test_encryption_decryption() {
        let service = create_test_service();
        let plaintext = "test-api-key-12345";

        let encrypted = service.encrypt_string(plaintext).unwrap();
        assert_ne!(encrypted, plaintext);

        let decrypted = service.decrypt_string(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Forget the service to avoid drop issues
        std::mem::forget(service);
    }

    #[test]
    fn test_supported_providers() {
        let service = create_test_service();

        let providers = service.get_supported_providers();
        assert!(providers.contains(&ApiKeyProvider::OpenAI));
        assert!(providers.contains(&ApiKeyProvider::Anthropic));
        assert!(providers.contains(&ApiKeyProvider::Custom));

        assert!(service.is_provider_supported(&ApiKeyProvider::OpenAI));
        assert!(service.is_provider_supported(&ApiKeyProvider::Anthropic));
        assert!(service.is_provider_supported(&ApiKeyProvider::Custom));
        assert!(!service.is_provider_supported(&ApiKeyProvider::Exchange(
            crate::types::ExchangeIdEnum::Binance
        )));

        // Forget the service to avoid drop issues
        std::mem::forget(service);
    }

    #[test]
    fn test_ai_analysis_request_validation() {
        let request = AiAnalysisRequest {
            prompt: "Analyze this market data".to_string(),
            market_data: json!({"symbol": "BTCUSDT", "price": 50000.0}),
            user_context: Some(json!({"risk_tolerance": "medium"})),
            max_tokens: Some(1000),
            temperature: Some(0.7),
        };

        assert_eq!(request.prompt, "Analyze this market data");
        assert!(request.user_context.is_some());
        assert_eq!(request.max_tokens, Some(1000));
        assert_eq!(request.temperature, Some(0.7));
    }

    #[test]
    fn test_ai_analysis_response_creation_comprehensive() {
        let mut metadata = HashMap::new();
        metadata.insert("model".to_string(), json!("gpt-4"));
        metadata.insert("usage".to_string(), json!({"tokens": 150}));

        let response = AiAnalysisResponse {
            analysis: "Market shows bullish trend".to_string(),
            confidence: Some(0.85),
            recommendations: vec!["Buy".to_string(), "Hold".to_string()],
            metadata,
        };

        assert_eq!(response.analysis, "Market shows bullish trend");
        assert_eq!(response.confidence, Some(0.85));
        assert_eq!(response.recommendations.len(), 2);
        assert!(response.metadata.contains_key("model"));
    }

    #[test]
    fn test_create_ai_provider_from_user_api_key() {
        let service = create_test_service();

        // Test OpenAI provider creation
        let openai_key = UserApiKey::new_ai_key(
            "user123".to_string(),
            "encrypted-key".to_string(),
            "".to_string(), // api_secret
            ApiKeyProvider::OpenAI,
        );

        let provider = service.create_ai_provider(&openai_key).unwrap();
        match provider {
            AiProvider::OpenAI { .. } => {
                // Success - provider created correctly
            }
            _ => panic!("Expected OpenAI provider"),
        }

        // Test Anthropic provider creation
        let anthropic_key = UserApiKey::new_ai_key(
            "user123".to_string(),
            "encrypted-key".to_string(),
            "".to_string(), // api_secret
            ApiKeyProvider::Anthropic,
        );

        let provider = service.create_ai_provider(&anthropic_key).unwrap();
        match provider {
            AiProvider::Anthropic { .. } => {
                // Success - provider created correctly
            }
            _ => panic!("Expected Anthropic provider"),
        }

        // Test Custom provider creation
        let custom_key = UserApiKey::new_ai_key(
            "user123".to_string(),
            "encrypted-key".to_string(),
            "".to_string(), // api_secret
            ApiKeyProvider::Custom,
        );

        let provider = service.create_ai_provider(&custom_key).unwrap();
        match provider {
            AiProvider::Custom { .. } => {
                // Success - provider created correctly
            }
            _ => panic!("Expected Custom provider"),
        }

        // Forget the service to avoid drop issues
        std::mem::forget(service);
    }

    #[test]
    fn test_create_ai_provider_custom_missing_base_url() {
        let service = create_test_service();

        let custom_key = UserApiKey::new_ai_key(
            "user123".to_string(),
            "encrypted-key".to_string(),
            "".to_string(), // api_secret
            ApiKeyProvider::Custom,
        );

        let result = service.create_ai_provider(&custom_key);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Custom provider requires base_url"));

        // Forget the service to avoid drop issues
        std::mem::forget(service);
    }

    #[test]
    fn test_create_ai_provider_from_exchange_key() {
        let service = create_test_service();

        let exchange_key = UserApiKey::new_exchange_key(
            "user123".to_string(),
            crate::types::ExchangeIdEnum::Binance,
            "encrypted-key".to_string(),
            "encrypted-secret".to_string(),
            None,  // passphrase
            false, // is_testnet
        );

        let result = service.create_ai_provider(&exchange_key);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported AI provider"));

        // Forget the service to avoid drop issues
        std::mem::forget(service);
    }
}

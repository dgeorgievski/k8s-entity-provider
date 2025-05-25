use serde_aux::field_attributes::deserialize_number_from_string;
use std::convert::{TryFrom, TryInto};
use std::collections::HashMap;
use url::Url;
use anyhow::Context;
use crate::backstage::entities;
use crate::errors::{ConfigError, Result};
#[derive(serde::Deserialize, Debug, Clone)]
pub struct Settings {
    pub name: String,
    pub display: String,
    pub cluster: String,
    pub server: ServerSettings,
    pub backstage: BackstageSettings,
    pub nats: NatsProxy,
    pub kube: KubeSettings,
    pub cache: Cache,
}

impl Settings {
    /// Validate all settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate name is not empty
        if self.name.is_empty() {
            return Err(ConfigError::missing("name"));
        }

        // Validate display is not empty
        if self.display.is_empty() {
            return Err(ConfigError::missing("display"));
        }

        // Validate cluster is not empty
        if self.cluster.is_empty() {
            return Err(ConfigError::missing("cluster"));
        }

        // Validate server settings
        self.server.validate()?;

        // Validate backstage settings
        self.backstage.validate()?;

        // Validate NATS settings
        self.nats.validate()?;

        // Validate Kubernetes settings
        self.kube.validate()?;

        // Validate cache settings
        self.cache.validate()?;

        Ok(())
    }
}
#[derive(serde::Deserialize, Debug, Clone)]
pub struct Cache {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub def_channel_size: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub poll_interval: u64,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub purge_cache_interval: u64,
}

impl Cache {
    /// Validate cache settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate def_channel_size is reasonable
        if self.def_channel_size == 0 {
            return Err(ConfigError::invalid(
                "cache.def_channel_size",
                "0".to_string(),
            ));
        }

        // Validate poll_interval is reasonable
        if self.poll_interval == 0 {
            return Err(ConfigError::invalid(
                "cache.poll_interval",
                "0".to_string(),
            ));
        }

        // Validate purge_cache_interval is reasonable
        if self.purge_cache_interval == 0 {
            return Err(ConfigError::invalid(
                "cache.purge_cache_interval",
                "0".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct NatsProxy {
    pub proxy_url: String
}

impl NatsProxy {
    /// Validate NATS proxy settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate proxy_url is not empty
        if self.proxy_url.is_empty() {
            return Err(ConfigError::missing("nats.proxy_url"));
        }

        // Validate proxy_url is a valid URL
        Url::parse(&self.proxy_url)
            .map_err(|e| ConfigError::invalid(
                "nats.proxy_url",
                format!("{}: {}", self.proxy_url, e),
            ))?;

        Ok(())
    }
}

/// Server rate limiting configuration
#[derive(serde::Deserialize, Debug, Clone)]
pub struct RateLimitSettings {
    /// Number of requests allowed per second per IP
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub requests_per_second: u32,
    
    /// Burst capacity for rate limiting
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub burst_size: u32,
    
    /// Whether to enable rate limiting
    #[serde(default = "default_rate_limit_enabled")]
    pub enabled: bool,
}

fn default_rate_limit_enabled() -> bool {
    true
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            burst_size: 200,
            enabled: true,
        }
    }
}

/// CORS configuration settings
#[derive(serde::Deserialize, Debug, Clone)]
pub struct CorsSettings {
    /// List of allowed origins, e.g. "https://example.com"
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    
    /// Whether to allow all origins (sets Access-Control-Allow-Origin: *)
    #[serde(default = "default_allow_all_origins")]
    pub allow_all_origins: bool,
    
    /// List of allowed methods, e.g. ["GET", "POST"]
    #[serde(default = "default_allowed_methods")]
    pub allowed_methods: Vec<String>,
    
    /// List of allowed headers
    #[serde(default = "default_allowed_headers")]
    pub allowed_headers: Vec<String>,
    
    /// Whether to allow credentials
    #[serde(default)]
    pub allow_credentials: bool,
    
    /// Max age for preflight requests in seconds
    #[serde(default = "default_max_age")]
    pub max_age: u32,
    
    /// Whether to enable CORS
    #[serde(default = "default_cors_enabled")]
    pub enabled: bool,
}

fn default_allow_all_origins() -> bool {
    false
}

fn default_allowed_methods() -> Vec<String> {
    vec![
        "GET".to_string(), 
        "POST".to_string(), 
        "PUT".to_string(), 
        "DELETE".to_string()
    ]
}

fn default_allowed_headers() -> Vec<String> {
    vec!["Content-Type".to_string(), "Authorization".to_string()]
}

fn default_max_age() -> u32 {
    86400 // 24 hours
}

fn default_cors_enabled() -> bool {
    true
}

impl Default for CorsSettings {
    fn default() -> Self {
        Self {
            allowed_origins: Vec::new(),
            allow_all_origins: false,
            allowed_methods: default_allowed_methods(),
            allowed_headers: default_allowed_headers(),
            allow_credentials: false,
            max_age: default_max_age(),
            enabled: true,
        }
    }
}
#[derive(serde::Deserialize,  Debug, Clone)]
pub struct ServerSettings {
    /// HTTP server port
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    
    /// HTTP server hostname
    pub host: String,
    
    /// Request timeout in seconds
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_request_timeout")]
    pub request_timeout: u64,
    
    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitSettings,
    
    /// CORS configuration
    #[serde(default)]
    pub cors: CorsSettings,
    
    /// Whether to enable request ID tracking
    #[serde(default = "default_request_id_enabled")]
    pub enable_request_id: bool,
}

fn default_request_timeout() -> u64 {
    30 // 30 seconds
}

fn default_request_id_enabled() -> bool {
    true
}

impl ServerSettings {
    /// Validate server settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate host is not empty
        if self.host.is_empty() {
            return Err(ConfigError::missing("server.host"));
        }

        // Validate port is in a reasonable range
        if self.port == 0 {
            return Err(ConfigError::invalid(
                "server.port",
                "0".to_string(),
            ));
        }

        // Validate request timeout
        if self.request_timeout == 0 {
            return Err(ConfigError::invalid(
                "server.request_timeout",
                "0".to_string(),
            ));
        }

        // Validate rate limiting settings
        if self.rate_limit.enabled {
            if self.rate_limit.requests_per_second == 0 {
                return Err(ConfigError::invalid(
                    "server.rate_limit.requests_per_second",
                    "0".to_string(),
                ));
            }

            if self.rate_limit.burst_size == 0 {
                return Err(ConfigError::invalid(
                    "server.rate_limit.burst_size",
                    "0".to_string(),
                ));
            }
        }

        // Validate CORS settings
        if self.cors.enabled && !self.cors.allow_all_origins && self.cors.allowed_origins.is_empty() {
            return Err(ConfigError::invalid(
                "server.cors.allowed_origins",
                "No allowed origins specified and allow_all_origins is false".to_string(),
            ));
        }

        Ok(())
    }
}
#[derive(serde::Deserialize,  Debug, Clone)] 
pub struct BackstageSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub name: String,
    pub annotations: Option<HashMap<String, String>>,
    pub groups: Vec<entities::Group>,
    pub users: Vec<entities::User>,
    pub domains: Option<Vec<entities::Domain>>
}

impl BackstageSettings {
    /// Validate backstage settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate name is not empty
        if self.name.is_empty() {
            return Err(ConfigError::missing("backstage.name"));
        }

        // Validate there is at least one user
        if self.users.is_empty() {
            return Err(ConfigError::invalid(
                "backstage.users",
                "must have at least one user".to_string(),
            ));
        }

        Ok(())
    }
}

/// Kubernetes client retry settings
#[derive(serde::Deserialize, Debug, Clone)]
pub struct KubeRetrySettings {
    /// Maximum number of retry attempts
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_max_retries")]
    pub max_retries: u32,
    
    /// Base delay for exponential backoff in milliseconds
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_base_delay_ms")]
    pub base_delay_ms: u64,
    
    /// Maximum delay for exponential backoff in milliseconds
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_max_delay_ms")]
    pub max_delay_ms: u64,
    
    /// Whether to enable retries
    #[serde(default = "default_retry_enabled")]
    pub enabled: bool,
}

fn default_max_retries() -> u32 {
    3
}

fn default_base_delay_ms() -> u64 {
    100
}

fn default_max_delay_ms() -> u64 {
    5000 // 5 seconds
}

fn default_retry_enabled() -> bool {
    true
}

impl Default for KubeRetrySettings {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            base_delay_ms: default_base_delay_ms(),
            max_delay_ms: default_max_delay_ms(),
            enabled: default_retry_enabled(),
        }
    }
}

/// Kubernetes connection pool settings
#[derive(serde::Deserialize, Debug, Clone)]
pub struct KubeConnectionSettings {
    /// Connection pool size
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_pool_size")]
    pub pool_size: usize,
    
    /// Connection idle timeout in seconds
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    
    /// Connection keep alive interval in seconds
    #[serde(deserialize_with = "deserialize_number_from_string", default = "default_keep_alive_secs")]
    pub keep_alive_secs: u64,
}

fn default_pool_size() -> usize {
    10
}

fn default_idle_timeout_secs() -> u64 {
    90 // 90 seconds
}

fn default_keep_alive_secs() -> u64 {
    30 // 30 seconds
}

impl Default for KubeConnectionSettings {
    fn default() -> Self {
        Self {
            pool_size: default_pool_size(),
            idle_timeout_secs: default_idle_timeout_secs(),
            keep_alive_secs: default_keep_alive_secs(),
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct KubeSettings {
    /// Whether to use TLS for Kubernetes API connection
    pub use_tls: bool,
    
    /// Resources to watch
    pub resources: Vec<Resource>,
    
    /// Retry settings
    #[serde(default)]
    pub retry: KubeRetrySettings,
    
    /// Connection pool settings
    #[serde(default)]
    pub connection: KubeConnectionSettings,
}

impl KubeSettings {
    /// Validate Kubernetes settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate resources
        for (i, resource) in self.resources.iter().enumerate() {
            resource.validate()
                .map_err(|e| ConfigError::invalid(
                    format!("kube.resources[{}]", i),
                    e.to_string(),
                ))?;
        }

        Ok(())
    }
}

impl Default for KubeSettings {
    fn default() -> KubeSettings {
        Self {
            use_tls: false,
            resources: Vec::new(),
            retry: KubeRetrySettings::default(),
            connection: KubeConnectionSettings::default(), 
        } 
    }
}

/// Kubernetes resource to watch
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Resource {
    /// Name of the resource type (e.g., "pods", "deployments")
    pub name: String,
    
    /// List of namespaces to watch this resource in
    pub namespaces: Vec<String>,
    
    /// Optional API groups for the resource
    pub api_groups: Option<Vec<String>>,
    
    /// Label selectors to filter resources
    pub label_selectors: Vec<String>,
    
    /// Field selectors to filter resources
    pub field_selectors: Vec<String>,
    
    /// Event type for this resource
    pub event_type: String,
}

impl Resource {
    /// Validate resource settings
    pub fn validate(&self) -> std::result::Result<(), ConfigError> {
        // Validate name is not empty
        if self.name.is_empty() {
            return Err(ConfigError::missing("resource.name"));
        }

        // Validate event_type is not empty
        if self.event_type.is_empty() {
            return Err(ConfigError::missing("resource.event_type"));
        }

        // Validate API groups if present
        if let Some(groups) = &self.api_groups {
            for (i, group) in groups.iter().enumerate() {
                if group.is_empty() {
                    return Err(ConfigError::invalid(
                        format!("resource.api_groups[{}]", i),
                        "Empty API group".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }
}

impl Default for Resource {
    fn default() -> Resource {
        Self {
            name: String::from("events"),
            namespaces: Vec::new(),
            api_groups: None,
            label_selectors: Vec::new(),
            field_selectors: Vec::new(),
            event_type: String::from("axyom.k8s.event.v1"),
        }
    }
}

/// Load application configuration from files and environment variables
/// 
/// This function loads settings from:
/// - A base.yaml file in the config directory
/// - An environment-specific yaml file (local.yaml or production.yaml)
/// - Environment variables prefixed with APP_ 
/// 
/// # Returns
/// A validated Settings object or an error
/// 
/// # Errors
/// Returns an error if:
/// - Configuration files can't be read
/// - Required settings are missing
/// - Settings have invalid values
/// - Settings deserialization fails
pub fn get_configuration() -> Result<Settings> {
    // Get current directory and configuration path
    let base_path = std::env::current_dir()
        .map_err(|e| ConfigError::IoError(e))?;
    let configuration_directory = base_path.join("config");

    // Detect the running environment.
    // Default to `local` if unspecified.
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .map_err(|e| ConfigError::env_var(e))?;

    let environment_filename = format!("{}.yaml", environment.as_str());
    let base_file_path = configuration_directory.join("base.yaml");
    let env_file_path = configuration_directory.join(&environment_filename);

    // Check if the configuration files exist
    if !base_file_path.exists() {
        return Err(ConfigError::IoError(
            std::io::Error::new(
                std::io::ErrorKind::NotFound, 
                format!("Configuration file not found: {:?}", base_file_path)
            )
        ).into());
    }

    if !env_file_path.exists() {
        tracing::warn!(
            "Environment configuration file not found: {:?}. Using only base settings.",
            env_file_path
        );
    }

    // Build configuration
    let builder = config::Config::builder()
        .add_source(config::File::from(base_file_path));

    // Add environment-specific file if it exists
    let builder = if env_file_path.exists() {
        builder.add_source(config::File::from(env_file_path))
    } else {
        builder
    };

    // Add environment variables
    let settings = builder
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("_"),
        )
        .build()
        .context("Failed to build configuration")?;

    // Deserialize settings
    let config: Settings = settings
        .try_deserialize()
        .context("Failed to deserialize configuration")?;

    // Validate settings
    config.validate()
        .context("Configuration validation failed")?;

    // Log success and return
    tracing::info!("Configuration loaded successfully for environment: {}", environment.as_str());
    Ok(config)
}

/// The possible runtime environment for our application.
pub enum Environment {
    Local,
    Production,
}

impl Environment {
    pub fn as_str(&self) -> &'static str {
        match self {
            Environment::Local => "local",
            Environment::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> std::result::Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),   
            other => Err(format!(
                "{} is not a supported environment. Use either `local` or `production`.",
                other
            )),
        }
    }
}

use std::fmt;
use std::result::Result as StdResult;
use thiserror::Error;
// use anyhow::Context as _;
use actix_web::{error::ResponseError, http::StatusCode, HttpResponse};

/// Application error types
#[derive(Error, Debug)]
pub enum AppError {
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// Kubernetes-related errors
    #[error("Kubernetes error: {0}")]
    Kubernetes(#[from] KubernetesError),

    /// HTTP/Server errors
    #[error("Server error: {0}")]
    Server(#[from] ServerError),

    /// Database/cache errors
    #[error("Database error: {0}")]
    Database(String),

    /// General application errors
    #[error("Application error: {0}")]
    Application(String),

    /// Unknown/unexpected errors
    #[error("Unknown error: {0}")]
    Unknown(#[from] anyhow::Error),
    
    /// Entity-related errors
    #[error("Entity error: {0}")]
    Entity(#[from] EntityError),
}

impl AppError {
    /// Create an application error with a message
    pub fn application<S: Into<String>>(msg: S) -> Self {
        Self::Application(msg.into())
    }

    /// Create a database error with a message
    pub fn database<S: Into<String>>(msg: S) -> Self {
        Self::Database(msg.into())
    }

    /// Convert this error into an anyhow error
    pub fn into_anyhow(self) -> anyhow::Error {
        anyhow::Error::new(self)
    }

    /// Add context to this error and convert to anyhow
    pub fn with_context<C, F>(self, f: F) -> anyhow::Error
    where
        C: fmt::Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        anyhow::Error::new(self).context(f())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        Self::Application(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        Self::Application(s.to_string())
    }
}

// Type alias for application Results
pub type Result<T> = StdResult<T, AppError>;

impl ResponseError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Kubernetes(_) => StatusCode::BAD_GATEWAY,
            AppError::Server(e) => match e {
                ServerError::ValidationError(_) => StatusCode::BAD_REQUEST,
                ServerError::RoutingError(_) => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Application(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Unknown(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Entity(_) => StatusCode::BAD_REQUEST,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .json(serde_json::json!({
                "error": self.to_string(),
                "code": self.status_code().as_u16()
            }))
    }
}

/// Configuration-related errors
#[derive(Error, Debug)]
pub enum ConfigError {
    /// Missing required configuration
    #[error("Missing required configuration: {0}")]
    MissingConfig(String),

    /// Invalid configuration value
    #[error("Invalid configuration value for {key}: {value}")]
    InvalidValue {
        key: String,
        value: String,
    },

    /// Environment variable error
    #[error("Environment variable error: {0}")]
    EnvVar(String),

    /// Failed to parse configuration
    #[error("Failed to parse configuration: {0}")]
    ParseError(String),

    /// I/O error while reading configuration
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    /// Other config errors
    #[error("Other configuration error: {0}")]
    Other(#[from] anyhow::Error),
}

impl ConfigError {
    /// Create a missing configuration error
    pub fn missing<S: Into<String>>(key: S) -> Self {
        Self::MissingConfig(key.into())
    }

    /// Create an invalid configuration value error
    pub fn invalid<K: Into<String>, V: Into<String>>(key: K, value: V) -> Self {
        Self::InvalidValue {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Create a parse error
    pub fn parse<S: Into<String>>(msg: S) -> Self {
        Self::ParseError(msg.into())
    }

    /// Create an environment variable error
    pub fn env_var<S: Into<String>>(msg: S) -> Self {
        Self::EnvVar(msg.into())
    }
}

/// Entity-specific errors
#[derive(Error, Debug)]
pub enum EntityError {
    /// Invalid entity type
    #[error("Invalid entity type: {0}")]
    InvalidType(String),
    
    /// Missing required field
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    /// Invalid field value
    #[error("Invalid field value: {field} = {value}")]
    InvalidValue {
        field: String,
        value: String,
    },
    
    /// Conversion error
    #[error("Entity conversion error: {0}")]
    ConversionError(String),
    
    /// Invalid configuration
    #[error("Invalid entity configuration: {0}")]
    InvalidConfig(String),
}

impl EntityError {
    /// Create an invalid type error
    pub fn invalid_type<S: Into<String>>(type_name: S) -> Self {
        Self::InvalidType(type_name.into())
    }
    
    /// Create a missing field error
    pub fn missing_field<S: Into<String>>(field: S) -> Self {
        Self::MissingField(field.into())
    }
    
    /// Create an invalid value error
    pub fn invalid_value<F: Into<String>, V: Into<String>>(field: F, value: V) -> Self {
        Self::InvalidValue {
            field: field.into(),
            value: value.into(),
        }
    }
    
    /// Create a conversion error
    pub fn conversion<S: Into<String>>(msg: S) -> Self {
        Self::ConversionError(msg.into())
    }
    
    /// Create an invalid config error
    pub fn invalid_config<S: Into<String>>(msg: S) -> Self {
        Self::InvalidConfig(msg.into())
    }
}

impl From<config::ConfigError> for ConfigError {
    fn from(err: config::ConfigError) -> Self {
        Self::ParseError(err.to_string())
    }
}

/// Kubernetes-specific errors
#[derive(Error, Debug)]
pub enum KubernetesError {
    /// Connection error to Kubernetes API
    #[error("Failed to connect to Kubernetes API: {0}")]
    ConnectionError(String),

    /// Authentication error
    #[error("Kubernetes authentication error: {0}")]
    AuthError(String),

    /// Resource not found
    #[error("Kubernetes resource not found: {kind} {namespace}/{name}")]
    ResourceNotFound {
        kind: String,
        namespace: String,
        name: String,
    },

    /// Error watching resources
    #[error("Error watching Kubernetes resources: {0}")]
    WatchError(String),

    /// Error from Kubernetes client library
    #[error("Kubernetes client error: {0}")]
    ClientError(#[from] kube::Error),

    /// Other Kubernetes errors
    #[error("Other Kubernetes error: {0}")]
    Other(#[from] anyhow::Error),
}

impl KubernetesError {
    /// Create a connection error
    pub fn connection<S: Into<String>>(msg: S) -> Self {
        Self::ConnectionError(msg.into())
    }

    /// Create an authentication error
    pub fn auth<S: Into<String>>(msg: S) -> Self {
        Self::AuthError(msg.into())
    }

    /// Create a resource not found error
    pub fn resource_not_found<K, N, NS>(kind: K, name: N, namespace: NS) -> Self 
    where
        K: Into<String>,
        N: Into<String>,
        NS: Into<String>,
    {
        Self::ResourceNotFound {
            kind: kind.into(),
            name: name.into(),
            namespace: namespace.into(),
        }
    }

    /// Create a watch error
    pub fn watch<S: Into<String>>(msg: S) -> Self {
        Self::WatchError(msg.into())
    }
}

impl From<http::Error> for KubernetesError {
    fn from(err: http::Error) -> Self {
        Self::ClientError(kube::Error::HttpError(err))
    }
}

/// HTTP/Server errors
#[derive(Error, Debug)]
pub enum ServerError {
    /// Binding to address failed
    #[error("Failed to bind to address: {0}")]
    BindError(#[from] std::io::Error),

    /// Routing error
    #[error("Routing error: {0}")]
    RoutingError(String),

    /// Serialization/Deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Request validation error
    #[error("Request validation error: {0}")]
    ValidationError(String),

    /// Unexpected internal server error
    #[error("Internal server error: {0}")]
    InternalError(String),

    /// Other server errors
    #[error("Other server error: {0}")]
    Other(#[from] anyhow::Error),
}

impl ServerError {
    /// Create a routing error
    pub fn routing<S: Into<String>>(msg: S) -> Self {
        Self::RoutingError(msg.into())
    }

    /// Create a serialization error
    pub fn serialization<S: Into<String>>(msg: S) -> Self {
        Self::SerializationError(msg.into())
    }

    /// Create a validation error
    pub fn validation<S: Into<String>>(msg: S) -> Self {
        Self::ValidationError(msg.into())
    }

    /// Create an internal server error
    pub fn internal<S: Into<String>>(msg: S) -> Self {
        Self::InternalError(msg.into())
    }
}

impl From<serde_json::Error> for ServerError {
    fn from(err: serde_json::Error) -> Self {
        Self::SerializationError(err.to_string())
    }
}

// We've removed our custom ResultExt to avoid conflicts with anyhow::Context
// Just use anyhow::Context directly for adding context to errors

/// Helper methods for working with Results
pub mod prelude {
    pub use super::{AppError, ConfigError, EntityError, KubernetesError, Result, ServerError};
    pub use anyhow::Context;
    
    /// Helper to map any error that implements Error + Send + Sync + 'static to anyhow::Error
    pub fn map_err_to_anyhow<T, E>(result: std::result::Result<T, E>) -> anyhow::Result<T>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        result.map_err(anyhow::Error::new)
    }
    
    /// Helper to map any error to AppError::Unknown
    pub fn map_err_to_app<T, E>(result: std::result::Result<T, E>) -> super::Result<T>
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        result.map_err(|e| AppError::Unknown(anyhow::Error::new(e)))
    }

    /// Helper to create an Ok(()) result
    pub fn ok() -> super::Result<()> {
        Ok(())
    }
}

use k8s_openapi::apimachinery::pkg::version;
use anyhow::{Context, Result};
use kube::{Client, Config, Error};
use std::convert::TryFrom;
use std::time::Duration;
use tokio::time::sleep;
use rand::Rng;
use std::sync::{Arc, Mutex};
use once_cell::sync::OnceCell;
use crate::configuration::KubeSettings;
use crate::errors::KubernetesError;

// Global client for connection pooling
static KUBE_CLIENT: OnceCell<Arc<Mutex<Option<Client>>>> = OnceCell::new();

/// Initialize the Kubernetes client with the given settings
///
/// # Arguments
/// * `settings` - Kubernetes settings
///
/// # Returns
/// A Result containing the client or an error
pub async fn initialize(settings: &KubeSettings) -> Result<()> {
    let client = create_client(settings).await?;
    
    // Initialize the global client
    let client_container = Arc::new(Mutex::new(Some(client)));
    KUBE_CLIENT.set(client_container)
        .map_err(|_| KubernetesError::connection("Failed to initialize Kubernetes client"))?;
    
    Ok(())
}

pub async fn client2(use_tls: bool) -> Result<Client, Error> {
    // init kube client
    let mut config = Config::infer().await.map_err(Error::InferConfig)?;
    if !use_tls {
        config.accept_invalid_certs = true;
    }

    Client::try_from(config)
}

/// Get the Kubernetes client
///
/// # Returns
/// A Result containing the client or an error
pub async fn client(settings: &KubeSettings) -> Result<Client> {
    // Try to get the global client first
    if let Some(client_container) = KUBE_CLIENT.get() {
        let guard = client_container.lock()
            .map_err(|_| KubernetesError::connection("Failed to acquire Kubernetes client lock"))?;
        
        if let Some(client) = guard.as_ref() {
            // Clone the client for the caller
            return Ok(client.clone());
        }
    }
    
    // No global client yet, create one with retry logic
    let retry_settings = &settings.retry;
    
    let mut attempt = 0;
    let mut last_error = None;
    
    while attempt <= retry_settings.max_retries {
        match create_client(settings).await {
            Ok(client) => {
                // If we made retries, log success
                if attempt > 0 {
                    tracing::info!("Successfully connected to Kubernetes API after {} retries", attempt);
                }
                
                // Initialize the global client if not already done
                if KUBE_CLIENT.get().is_none() {
                    let client_container = Arc::new(Mutex::new(Some(client.clone())));
                    // It's okay if this fails - someone else might have initialized it
                    let _ = KUBE_CLIENT.set(client_container);
                }
                
                return Ok(client);
            },
            Err(err) => {
                last_error = Some(err);
                
                // Don't retry if retries are disabled
                if !retry_settings.enabled {
                    break;
                }
                
                // Don't retry if we've reached the maximum number of retries
                if attempt >= retry_settings.max_retries {
                    break;
                }
                
                // Calculate backoff time with jitter
                let backoff_ms = calculate_backoff(
                    attempt, 
                    retry_settings.base_delay_ms, 
                    retry_settings.max_delay_ms
                );
                
                // Log the retry attempt
                tracing::warn!(
                    "Failed to connect to Kubernetes API (attempt {}/{}). Retrying in {}ms: {}",
                    attempt + 1,
                    retry_settings.max_retries,
                    backoff_ms,
                    last_error.as_ref().unwrap()
                );
                
                // Sleep for the backoff duration
                sleep(Duration::from_millis(backoff_ms)).await;
                
                // Increment the attempt counter
                attempt += 1;
            }
        }
    }
    
    // If we got here, we've exhausted our retries or encountered a non-retryable error
    Err(last_error.unwrap_or_else(|| 
        KubernetesError::connection("Failed to create Kubernetes client and no specific error was recorded").into()
    ))
}

/// Calculate backoff time with jitter for retry mechanism
/// 
/// This implements exponential backoff with jitter to avoid thundering herd problems.
/// The formula is: min(max_delay, base_delay * 2^attempt) + random_jitter
/// 
/// # Arguments
/// * `attempt` - Current attempt number (0-based)
/// * `base_delay_ms` - Base delay in milliseconds
/// * `max_delay_ms` - Maximum delay in milliseconds
/// 
/// # Returns
/// Backoff time in milliseconds
fn calculate_backoff(attempt: u32, base_delay_ms: u64, max_delay_ms: u64) -> u64 {
    // Calculate exponential backoff: base_delay * 2^attempt
    let exp_backoff = base_delay_ms.saturating_mul(2u64.saturating_pow(attempt));
    
    // Cap it at the maximum delay
    let capped_backoff = exp_backoff.min(max_delay_ms);
    
    // Add jitter: random value between 0 and 25% of the backoff
    let jitter_range = (capped_backoff / 4).max(1);
    let jitter = rand::rng().random_range(0..jitter_range);
    
    capped_backoff.saturating_add(jitter)
}

/// Create a new Kubernetes client with the given settings
/// 
/// # Arguments
/// * `settings` - Kubernetes settings
/// 
/// # Returns
/// A Result containing the client or an error
async fn create_client(settings: &KubeSettings) -> Result<Client> {
    // Try to infer Kubernetes config from the environment
    let mut config = Config::infer().await
        .context("Failed to infer Kubernetes configuration")?;
    
    // Apply TLS settings
    if !settings.use_tls {
        config.accept_invalid_certs = true;
    }
    
    
    
    // Configure connection timeouts
    let timeout = Duration::from_secs(30); // Default timeout
    config.connect_timeout = Some(timeout);
    config.read_timeout = Some(timeout);
    config.write_timeout = Some(timeout);
    
    // Configure connection settings if using tokio runtime
    // TODO review if this is still needed
    #[cfg(feature = "runtime")]
    {
        use kube::client::ConfigExt;
        // Apply connection pool settings
        let conn_settings = &settings.connection;
        // Set connection pool settings
        config = config
            .maybe_client_qps(Some(5.0)) // Limit QPS to 5
            .maybe_client_burst(Some(10)) // Burst of 10
            .with_connect_timeout(Duration::from_secs(conn_settings.connect_timeout_secs))
            .with_read_timeout(Duration::from_secs(conn_settings.read_timeout_secs))
            .with_write_timeout(Duration::from_secs(conn_settings.write_timeout_secs));
    }


    // Create the client
    let client = Client::try_from(config)
        .context("Failed to create Kubernetes client from configuration")?;
    
    // Test the connection
    test_connection(&client).await?;
    
    Ok(client)
}

/// Test the connection to the Kubernetes API
/// 
/// # Arguments
/// * `client` - Kubernetes client
/// 
/// # Returns
/// A Result indicating if the connection is successful
async fn test_connection(client: &Client) -> Result<()> {
    // Try to get the API versions to verify connectivity
    let api_versions = client.apiserver_version().await
        .context("Failed to connect to Kubernetes API")?;
    
    tracing::debug!(
        "Connected to Kubernetes API: version={}, platform={}",
        api_versions.git_version,
        api_versions.platform
    );
    
    Ok(())
}

/// Close the Kubernetes client connection
/// 
/// This function should be called during application shutdown to properly
/// close the connection to the Kubernetes API.
pub async fn cleanup() -> Result<()> {
    if let Some(client_container) = KUBE_CLIENT.get() {
        let mut guard = client_container.lock()
            .map_err(|_| KubernetesError::connection("Failed to acquire Kubernetes client lock during cleanup"))?;
            
        // Replace the client with None to drop it
        *guard = None;
        
        tracing::info!("Kubernetes client connection closed");
    }
    
    Ok(())
}

/// Get the Kubernetes version
/// 
/// # Arguments
/// * `settings` - Kubernetes settings
/// 
/// # Returns
/// A Result containing the Kubernetes version
pub async fn get_version(settings: &KubeSettings) -> Result<version::Info> {
    let client = client(settings).await?;
    
    client.apiserver_version()
        .await
        .context("Failed to get Kubernetes API server version")
}

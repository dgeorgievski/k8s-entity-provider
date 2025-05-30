use crate::routes::{
    api::v1 as api_v1,
    health_check, 
    bs_provider_version};
use crate::configuration::Settings;
use crate::ax_types::Db;
use crate::backstage::entities;
use crate::errors::{AppError, ServerError, Result};
use actix_web::{web, 
    get, 
    App, 
    HttpServer, 
    HttpResponse,
    middleware};
use std::net::TcpListener;
use actix_web::dev::{
    ServerHandle, ServiceRequest, ServiceResponse};
use tracing_actix_web::{
    TracingLogger, 
    DefaultRootSpanBuilder, 
    RootSpanBuilder, 
    Level};
use actix_web::Error as ActixError;
use tracing::Span;
use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use tokio::signal;


/// Application state shared across all request handlers
pub struct ApplicationState {
    /// Application configuration
    pub config: Settings,
    /// Shared data cache
    pub cache: Db,
    /// Backstage groups
    pub groups: Arc<Vec<entities::Group>>,
    /// Backstage users
    pub users: Arc<Vec<entities::User>>,
    /// Backstage domains
    pub domains: Arc<Option<Vec<entities::Domain>>>,
}

impl ApplicationState {
    /// Create a new application state
    pub fn new(config: Settings, cache: Db) -> Self {
        let groups = Arc::new(entities::Group::groups_from_config(config.backstage.clone()));
        let users = Arc::new(entities::User::users_from_config(config.backstage.clone()));
        let domains = Arc::new(
            Some(entities::Domain::domains_from_config(
                config.backstage.clone())));
        
        Self {
            config,
            cache,
            groups,
            users,
            domains,
        }
    }
    
    /// Clean up any resources on shutdown
    pub async fn cleanup(&self) {
        tracing::info!("Cleaning up application resources");
        // Add cleanup logic here
    }
}

pub struct CustomLevelRootSpanBuilder;

impl RootSpanBuilder for CustomLevelRootSpanBuilder {
    fn on_request_start(request: &ServiceRequest) -> Span {
        let level = match request.path() {
            "/healthz" => Level::DEBUG,
            "/api/v1/entities" => Level::INFO,
            _ => Level::INFO
        };
        tracing_actix_web::root_span!(level = level, request)
    }

    fn on_request_end<B: actix_web::body::MessageBody>(
        span: Span, 
        outcome: &std::result::Result<ServiceResponse<B>, ActixError>
    ) {
        DefaultRootSpanBuilder::on_request_end(span, outcome);
    }
}

#[get("/")]
async fn index(data: web::Data<ApplicationState>) -> HttpResponse {
    let welcome = format!("Welcome to {}!", data.config.display);
    HttpResponse::Ok().body(welcome)
}

/// Run the application server
/// 
/// # Arguments
/// * `listener` - TCP listener for the server
/// * `conf` - Application configuration
/// * `cache` - Shared data cache
/// 
/// # Returns
/// A server instance that can be awaited
/// 
/// # Errors
/// Returns an error if the server fails to start
pub async fn run(
    listener: TcpListener, 
    conf: &Settings,
    cache: Db
) -> Result<impl Future<Output = std::io::Result<()>>> {
    // Create application state
    let app_state = ApplicationState::new(conf.clone(), cache);
    let app_state_data = web::Data::new(app_state);
    let app_state_data_closure = app_state_data.clone();

    // TODO find out how actix handles request timeouts
    // Define request timeout - default 30 seconds
    // let request_timeout = conf.server.request_timeout;
    // let timeout_duration = Duration::from_secs(request_timeout);

    // Create the server
    let server = HttpServer::new(move || {
        let api_v1 = web::scope("/api/v1")
            .app_data(app_state_data.clone())
            .service(web::resource("/entities").to(api_v1::entities::get_entities))
            .service(web::resource("/redis/status").to(api_v1::entities::redis_status));

        App::new()
            .app_data(app_state_data.clone())
            // Add logging middleware
            .wrap(TracingLogger::<CustomLevelRootSpanBuilder>::new())
            // Add common middleware for security and compression
            .wrap(middleware::Compress::default())
            .wrap(middleware::DefaultHeaders::new().add(("X-Content-Type-Options", "nosniff")))
            // Add services and routes
            .service(index)
            .service(bs_provider_version)
            .service(api_v1)
            .route("/healthz", web::get().to(health_check))
    })
    .listen(listener)
    .map_err(ServerError::BindError)?
    .workers(num_cpus::get()) // Set workers to number of CPU cores
    .shutdown_timeout(30)
    .run(); // Give 30 seconds for graceful shutdown

    let server_handle = server.handle();
    // Create a future that completes when shutdown signal is received
    // let server_handle = match server.run().await {
    //     Ok(handle) => handle,
    //     Err(e) => return Err(AppError::Server(ServerError::InternalError(
    //         format!("Failed to start server: {}", e),
    //     ))),
    // };
    
    // Create a future that handles graceful shutdown
    let shutdown_future = graceful_shutdown(server_handle, 
                                    app_state_data_closure.into_inner());

    match server.await {
        Ok(_) => {
            Ok(shutdown_future)
        },
        Err(e) => {
            Err(AppError::Server(ServerError::InternalError(e.to_string())))
        }
    }

    
}

/// Handles graceful shutdown of the server
/// 
/// Waits for a SIGTERM or SIGINT signal and then performs a graceful shutdown
/// of the server and application resources.
/// 
/// # Arguments
/// * `server` - Server future that is running
/// * `app_state` - Application state to clean up
/// 
/// # Returns
/// A future that resolves when the server has shut down
async fn graceful_shutdown(
    server_handle: ServerHandle,
    app_state: Arc<ApplicationState>,
) -> std::io::Result<()> {
    // Create a future that completes when a signal is received
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        tracing::info!("Received SIGINT (Ctrl+C) signal");
    };

    // Create a future for SIGTERM handling
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
        tracing::info!("Received SIGTERM signal");
    };

    // For non-unix systems, use a never-completing future for SIGTERM
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Wait for either signal
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    // Begin graceful shutdown
    tracing::info!("Starting graceful shutdown...");

    // Clean up application resources
    app_state.cleanup().await;

    // Shut down the server
    // The server will finish in-flight requests before shutting down
    // based on the shutdown_timeout we set
    let graceful_server_shutdown = server_handle.stop(true);
    
    // Set a timeout for the server shutdown
    let shutdown_timeout = Duration::from_secs(35); // 5 seconds more than server shutdown_timeout
    
    match tokio::time::timeout(shutdown_timeout, graceful_server_shutdown).await {
        Ok(_) => {
            tracing::info!("Server gracefully shut down");
            Ok(())
        },
        Err(err) => {
            tracing::warn!("Server shutdown timed out - forcing shutdown");
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("Server shutdown timed out: {}", err),
            ))
        }
    }
}

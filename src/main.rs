use k8s_entity_provider::startup::run;
use k8s_entity_provider::ax_types::Db;
use k8s_entity_provider::configuration::get_configuration;
use k8s_entity_provider::telemetry::{get_subscriber, init_subscriber};
use k8s_entity_provider::ax_kube::{utils, watch::watch};
use k8s_entity_provider::backstage::ingest;
use std::net::TcpListener;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Shared cache across threads
    let cache: Db = Arc::new(Mutex::new(BTreeMap::new()));

    let config = get_configuration().expect("Failed to read configuration");
    let subscriber = get_subscriber(config.name.clone(), "info".into(), std::io::stdout);
    init_subscriber(subscriber); 

    let k8s_version = match utils::get_k8s_version(&config).await {
        Ok(sv) => {
            format!("{0}.{1}", sv.major, sv.minor)
        },
        Err(_) => "n/a".to_owned()
    };
    
    tracing::info!("k8s: {0}", k8s_version);
    
    // start thread for watching targetted k8s resources
    match watch(&config, k8s_version.clone()).await {
        Ok(events_channels) => {
            let _ = ingest::process_k8s_resources(&config, 
                                                events_channels, 
                                                cache.clone()).await;
        },
        Err(why) => {
            tracing::error!("Failed to watch configured resources {:?}", why);
        }
    };

    let address = format!(
        "{}:{}",
        config.server.host, 
        config.server.port
    );
    let listener = TcpListener::bind(address)?;
    match run(listener, &config, cache.clone()).await {
        Ok(_) => tracing::info!("Server gracefully shut down"),
        Err(e) => tracing::error!("Server shutdown timed out: {}", e),
    }

    Ok(())
}

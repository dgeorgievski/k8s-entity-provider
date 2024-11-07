use serde_aux::field_attributes::deserialize_number_from_string;
use std::convert::{TryFrom, TryInto};
use std::collections::HashMap;
use crate::backstage::entities;

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

#[derive(serde::Deserialize,  Debug, Clone)]
pub struct Cache {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub def_channel_size: usize,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub poll_interval: u64,
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub purge_cache_interval: u64,
}

#[derive(serde::Deserialize,  Debug, Clone)]
pub struct NatsProxy {
    pub proxy_url: String
}

#[derive(serde::Deserialize,  Debug, Clone)]
pub struct ServerSettings {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub port: u16,
    pub host: String,
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

#[derive(serde::Deserialize, Debug, Clone)]
pub struct KubeSettings {
    pub use_tls: bool,
    pub resources: Vec<Resource>,
}

impl Default for KubeSettings {
    fn default() -> KubeSettings {
        Self {
            use_tls: false,
            resources: Vec::new(),
        } 
    }
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Resource {
    pub name: String,
    pub namespaces: Vec<String>,
    pub api_groups: Option<Vec<String>>,
    pub label_selectors: Vec<String>,
    pub field_selectors: Vec<String>,
    pub event_type: String,
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

pub fn get_configuration() -> Result<Settings, config::ConfigError> {
    let base_path = std::env::current_dir().expect("Failed to determine the current directory");
    let configuration_directory = base_path.join("config");

    // Detect the running environment.
    // Default to `local` if unspecified.
    let environment: Environment = std::env::var("APP_ENVIRONMENT")
        .unwrap_or_else(|_| "local".into())
        .try_into()
        .expect("Failed to parse APP_ENVIRONMENT.");

    let environment_filename = format!("{}.yaml", environment.as_str());
    let settings = config::Config::builder()
        .add_source(config::File::from(
            configuration_directory.join("base.yaml"),
        ))
        .add_source(config::File::from(
            configuration_directory.join(environment_filename),
        ))
        // Add in settings from environment variables (with a prefix of APP and '__' as separator)
        // E.g. `APP_APPLICATION__PORT=5001 would set `Settings.application.port`
        .add_source(
            config::Environment::with_prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()?;

    match settings.try_deserialize::<Settings>() {
        Err(why) => {
                tracing::error!("failed to load config");
                return Err(why)
            },
        Ok(config) => {
            // dbg!(&config);
            return Ok(config)
        },     
    }
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

    fn try_from(s: String) -> Result<Self, Self::Error> {
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

mod apigroup;
mod discovery;
pub mod utils;
pub mod client;
pub mod dynamic_object;
pub mod watch;
pub mod watch_event;

pub use client::client;
pub use discovery::new;
pub use discovery::{dynamic_api, resolve_api_resources};
pub use watch::watch;
pub use watch_event::WatchEvent;



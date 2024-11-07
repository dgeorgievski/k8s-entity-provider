use kube::api::DynamicObject;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct WatchEvent{
    pub k8s_version: String,
    pub resource_url:  String,
    pub event_type: String,
    // pub dynamic_object: Option<DynamicObject>,
    pub command: WatchCommand,
}

impl Default for WatchEvent {
    fn default() -> Self {
        WatchEvent{
            k8s_version: "".to_owned(),
            resource_url: "".to_owned(),
            event_type: "".to_owned(),
            command: WatchCommand::PrintAll,
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub enum WatchCommand {
    Add(DynamicObject),
    Delete(DynamicObject),
    Update(DynamicObject),
    PrintAll,
    Purge,
    None,
}
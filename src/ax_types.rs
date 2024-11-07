use kube::api::DynamicObject;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub type Db = Arc<Mutex<BTreeMap<String, DynamicObject>>>;
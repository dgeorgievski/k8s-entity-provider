use std::collections::HashMap;

use actix_web::{web, Result, Responder};
use kube::ResourceExt;
use serde_json::Value;
use crate::backstage::entities;
use crate::configuration::Settings;
use crate::ax_types::Db;

#[derive(Debug)]
pub enum K8sKinds {
    StatefulSet,
    Deployment,
    Pod,
    Unknown,
}

impl K8sKinds {
    fn get_kind(name: &String) -> Self {
        match name.to_lowercase().as_str() {
            "statefulset" => K8sKinds::StatefulSet,
            "deployment" => K8sKinds::Deployment,
            "pod" => K8sKinds::Pod,
            _ => K8sKinds::Unknown,
        }
    }
}

pub async fn get_entities(web_config: web::Data<Settings>, 
    groups: web::Data<Vec<entities::Group>>,
    users: web::Data<Vec<entities::User>>,
    domains: web::Data<Vec<entities::Domain>>,
    cache: web::Data<Db>) -> Result<impl Responder> {
    // HttpResponse {
        // Result<impl Responder>
    let db = cache.lock().unwrap();
    
    // let mut res: Vec<entities:::Resource> = Vec::new();
    let mut res: Vec<Box<dyn entities::BackstageEntity>> = Vec::new();
    let mut seen: HashMap<String, entities::Resource> = HashMap::new();
    let mut seen_system: HashMap<String, u8> = HashMap::new();
    for (_, obj) in db.iter() {
        let obj_kind: K8sKinds = match &obj.types {
            Some(t) => {
                K8sKinds::get_kind(&t.kind)
            },
            None => {
                tracing::debug!("unknown k8s resource {:?}", obj.name_any());
                continue;
            }
        };

        match obj_kind {
            K8sKinds::StatefulSet => {
                // Create Resource for Redis Shard
                let redis_shard = match entities::Resource::redis_shard_from_statefulset(&web_config, obj){
                    Ok(res) => res,
                    Err(why) => {
                        tracing::error!("Resource Entity conversion failed {:?}", why);
                        continue;
                    }
                };
                res.push(Box::new(redis_shard.clone()));

                // Create Redis cluster Resource
                match entities::Resource::redis_cluster_from_shard(&web_config, redis_shard) {
                    Ok(cluster) => {
                        let sname = format!("redis_cluster/{}", cluster.metadata.name.clone());
                        match seen.get_mut(&sname) {
                            Some(seen_cluster) => {
                                // append new dependencies to seen cluster's dependencies
                                let mut dep_new = cluster.spec.depends_on.clone().unwrap();
                                let mut dep_seen = seen_cluster.spec.depends_on.clone().unwrap();
                                dep_seen.append(&mut dep_new);
                                seen_cluster.spec.depends_on = Some(dep_seen);
                            },
                            None => {
                                seen.insert(sname, cluster);
                            },
                        }
                    },
                    Err(why) => {
                        tracing::error!("System Entity conversion failed {:?}", why);
                    }
                }

                // create System for the Redis cluster
                match entities::System::from_stateful_set(&web_config, obj) {
                    Ok(system) => {
                        let sname = format!("system/{}", system.metadata.name.clone());
                        if seen_system.contains_key(&sname) {
                            continue;
                        }else{
                            seen_system.insert(sname, 1);
                        }
                        res.push(Box::new(system));
                    },
                    Err(why) => {
                        tracing::error!("System Entity conversion failed {:?}", why);
                    },
                }
                
            },
            K8sKinds::Pod => {
                let redis_node = match entities::Resource::redis_node_from_pod(&web_config, obj){
                    Ok(node) => node,
                    Err(why) => {
                        tracing::error!("Resource Entity conversion failed {:?}", why);
                        continue;
                    }
                };
                res.push(Box::new(redis_node.clone()));
            },
            K8sKinds::Deployment => {
                tracing::debug!("k8s kind coming soon: {:?}", obj_kind);
            },
            _ => {
                tracing::debug!("k8s kind not supported: {:?}", obj_kind);
            }
        }
    }

    for (_key, redis_cluster) in seen {
        res.push(Box::new(redis_cluster.clone()));
    }

    if !groups.is_empty() {
        for g in groups.iter() {
            res.push(Box::new(g.clone()));
        }
    }

    if !users.is_empty() {
        for u in users.iter() {
            res.push(Box::new(u.clone()));
        }
    }

    if !domains.is_empty() {
        for d in domains.iter() {
            res.push(Box::new(d.clone()));
        }
    }

    Ok(web::Json(res))
}
#[derive(serde::Serialize)]
struct RedisStatus {
    name: String,
    namespace: String,
    cluster: String,
    available_replicas: Value,
    collision_count: Value,
    current_replicas: Value,
    current_revision: Value,
    observed_generation: Value,
    ready_replicas: Value,
    replicas: Value,
    update_revision: Value,
    updated_replicas: Value,
}

// return status of Redis StatefulSets clusters
pub async fn redis_status(cache: web::Data<Db>) ->Result<impl Responder> {
    let db = cache.lock().unwrap();    
    let mut res: Vec<RedisStatus> = Vec::new();
    for (_, obj) in db.iter() {

        match &obj.types {
            Some(tp) => {
                if tp.kind.to_lowercase() != "statefulset" {
                    continue;
                }
            },
            None => continue,
        }

        let labels = obj.labels();
        if let Some(lval) = labels.get("app.kubernetes.io/component") {
            if lval == "redis-cluster" {
                let ns = match &obj.metadata.namespace {
                    Some(ns) => ns.clone(),
                    None => "".to_string(),
                };

                let status = match obj.data.get("status") {
                    Some(Value::Object(st)) => st,
                    Some(_) => continue,
                    None => continue,
                };

                res.push(RedisStatus{
                    name: obj.name_any(),
                    namespace: ns,
                    cluster: "ci-cd".to_string(),
                    available_replicas: status.get("availableReplicas").unwrap().to_owned(),
                    collision_count: status.get("collisionCount").unwrap().to_owned(),
                    current_replicas: status.get("currentReplicas").unwrap().to_owned(),
                    current_revision: status.get("currentRevision").unwrap().to_owned(),
                    observed_generation: status.get("observedGeneration").unwrap().to_owned(),
                    ready_replicas: status.get("readyReplicas").unwrap().to_owned(),
                    replicas: status.get("replicas").unwrap().to_owned(),
                    update_revision: status.get("updateRevision").unwrap().to_owned(),
                    updated_replicas: status.get("updatedReplicas").unwrap().to_owned(),
                });
            }
        }
    }

    Ok(web::Json(res))
}
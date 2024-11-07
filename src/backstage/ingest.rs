use std::collections::BTreeMap;

use regex::Regex;
use kube::core::{TypeMeta, DynamicObject};
use kube::api::ResourceExt;
use tokio::{
    sync::mpsc::{Sender, Receiver, channel},
    time::{self, Duration}
};

use anyhow::Result;
use crate::ax_types::Db;
use crate::ax_kube::{
    watch::{EventsChannels, check_objects}, 
    watch_event::{WatchCommand, WatchEvent}};
use crate::configuration::Settings;
use crate::backstage::{capitalize, format_creation_since};

// Cache reported k8s resource 
//rx_we: Receiver<WatchEvent>,
pub async fn process_k8s_resources(conf: &Settings, 
                        events_channels: EventsChannels,
                        cache: Db) -> Result<bool, regex::Error> {
    let (tx_api, rx_api): (Sender<String>, Receiver<String>) = channel(32);
    let (tx_type, rx_type): (Sender<Option<TypeMeta>>, Receiver<Option<TypeMeta>>) = channel(32);

    //todo improve error handling and passing
    let result = match parse_type_meta(rx_api, tx_type).await {
        Ok(_) => {
            let _result = process_watch_event(&conf, 
                                        events_channels, 
                                        tx_api, 
                                        rx_type,
                                        cache).await;
            true
        },
        Err(why) => {
            tracing::error!("Starting TypeMeta parser failed {:?}", why);
            false
        },
    };

    Ok(result)
}

// Receives a k8s API path and returns a TypeMeta structure.
// rx(/api/v1/events) -> parse -> sn(TypeMeta{ api_version: v1, kind: Event}) 
// todo add caching for a given path to avoid repeated regex matching.
pub async fn parse_type_meta(mut rx: Receiver<String>, 
                                tx: Sender<Option<TypeMeta>>) -> Result<bool, regex::Error> {
    let k8s_api_pattern = vec![
        r"/api/(?<ver>[a-z0-9]*)/(?<resource>[a-zA-Z0-9-]*)s$",
        r"/api/(?<ver>[a-z0-9]*)/namespaces/(?<ns>[a-zA-Z0-9-]*)/(?<resource>[a-zA-Z0-9]*)s$",
        r"/apis/(?<apigroup>[a-z0-9\.]*)/(?<ver>[a-z0-9]*)/(?<resource>[a-zA-Z0-9]*)s$",
        r"/apis/(?<apigroup>[a-z0-9\.]*)/(?<ver>[a-z0-9]*)/namespaces/(?<ns>[a-zA-Z0-9-]*)/(?<resource>[a-zA-Z0-9]*)s$",
    ];
        // /apis/apps/v1/namespaces/app-health-5g/deployments
    let mut k8s_api_rex: Vec<Regex> = Vec::new();

    for p in k8s_api_pattern {
        match Regex::new(p) {
            Ok(r) => {
                k8s_api_rex.push(r);
            },
            Err(err) => { 
                return Err(err)
            },
        }
    }

    tokio::spawn(async move {
        while let Some(hay) = rx.recv().await {
            let mut result: Option<TypeMeta> = None;

            'k8sapi: for r in &k8s_api_rex {
                if let Some(caps) = r.captures(&hay) {
                    // todo add apigrpup to api_version
                    let api_version = caps
                                    .name("apigroup")
                                    .map_or(caps["ver"].to_string(), 
                                        |v| format!("{}/{}", v.as_str(), 
                                                            caps["ver"].to_string()));

                    result = Some(TypeMeta{
                                // api_version: caps["ver"].to_string(),
                                api_version,
                                kind: capitalize(&caps["resource"]),
                            });          
                    // skip the remaining patterns
                    break 'k8sapi;
                };
            };

            if let Err(why) = tx.send(result).await{
                tracing::error!("Failed to send TypeMeta: {:?}", why);
            };
        };
    });

    Ok(true)
}

/*
Process WatchEvents stream
*/
// mut rx_we: Receiver<WatchEvent>,
pub async fn process_watch_event(conf: &Settings,
    events_channels: EventsChannels,
    tx_api: Sender<String>,
    mut rx_type: Receiver<Option<TypeMeta>>,
    cache: Db) -> std::io::Result<()> {

    let mut rx_we = events_channels.rx;
    let tx_poll = events_channels.tx.clone();
    let tx_purge = events_channels.tx.clone();
    let mut ipoll = time::interval(Duration::from_secs(conf.cache.poll_interval)); 
    let mut ipurge = time::interval(Duration::from_secs(conf.cache.purge_cache_interval)); 
    let conf2 = conf.clone();
    // ingest thread
    tokio::spawn(async move {  
        // println!("{0:<20} {1:<20} {2:<20} {3:<5} {4:<width$}", "KIND", "NAMESPACE", "AGE", "K8S", "NAME", width = 63);
        while let Some(we) = rx_we.recv().await {
            match we.command {
                WatchCommand::Add(obj) | WatchCommand::Update(obj) => {
                    let obj_to_add = match process_dynobj(obj.clone(),
                                            we.resource_url.clone(),
                                            tx_api.clone(),
                                            &mut rx_type).await {
                        Ok(obj) => obj,
                        Err(why) => {
                            tracing::error!("processing dynobj failed: {:?}", why);
                            continue
                        }
                    };

                    let name = obj_to_add.name_any().clone();
                    let ns = match obj_to_add.metadata.namespace.clone() {
                        Some(ref namespace) => namespace.to_string(),
                        None => "none".to_string(),
                    };
                
                    let tm_kind = match obj_to_add.types {
                        Some(ref tm) => tm.kind.clone(),
                        None => "none".to_owned(),
                    };

                    let age = format_creation_since(obj_to_add.creation_timestamp());
                    let key = &format!("{}/{}", ns, name);
                    let mut db = cache.lock().unwrap();
                    // insert or update DynamicObject in the cash
                    db.insert(key.to_string(), obj_to_add);

                    println!(" >> DB ins {0:<20} {1:<20} {2:<20} {3:<5} {4:<width$}", 
                                tm_kind, 
                                ns.clone(), 
                                age, 
                                we.k8s_version,
                                name, 
                                width = 80);
                },
                WatchCommand::Delete(obj) => {
                    let name = obj.name_any().clone();
                    let ns = match obj.metadata.namespace {
                        Some(ref namespace) => namespace.to_string(),
                        None => "none".to_string(),
                    };
                
                    let tm_kind = match obj.types {
                        Some(ref tm) => tm.kind.clone(),
                        None => "none".to_owned(),
                    };

                    let age = format_creation_since(obj.creation_timestamp());

                    let mut db = cache.lock().unwrap();
                    let key = &format!("{}/{}", ns, name);
                    db.remove(key);

                    println!(" >> DB del {0:<20} {1:<20} {2:<20} {3:<5} {4:<width$}", 
                                        tm_kind, 
                                        ns.clone(), 
                                        age, 
                                        we.k8s_version,
                                        name, 
                                        width = 80);
                },
                WatchCommand::Purge => {
                    let mut db: BTreeMap<String, DynamicObject> = BTreeMap::new();
                    cache.lock().unwrap().clone_into(&mut db);
                    let mut check_objs: Vec<DynamicObject> = Vec::new();
                    for (_, obj) in db.iter(){
                        check_objs.push(obj.clone());
                    }
                    
                    // find inactive objects
                    let objs = match check_objects(check_objs, &conf2).await {
                        Ok(objs) => objs,
                        Err(_)=> vec![],
                    };

                    let mut db = cache.lock().unwrap();
                    for obj in objs.iter() {
                        let name = obj.name_any().clone();
                        let ns = match obj.metadata.namespace {
                            Some(ref namespace) => namespace.to_string(),
                            None => "none".to_string(),
                        };
                    
                        let tm_kind = match obj.types {
                            Some(ref tm) => tm.kind.clone(),
                            None => "none".to_owned(),
                        };

                        let age = format_creation_since(obj.creation_timestamp());

                        
                        let key = &format!("{}/{}", ns, name);
                    
                        db.remove(key);

                        println!(" >> DB purge {0:<20} {1:<20} {2:<20} {3:<5} {4:<width$}", 
                                            tm_kind, 
                                            ns.clone(), 
                                            age, 
                                            we.k8s_version,
                                            name, 
                                            width = 80);
                    }
                },
                WatchCommand::PrintAll => {
                    println!("\n>> Printing Cache DynamicObjects");
                    let db = cache.lock().unwrap();
                    for (_, obj) in db.iter() {
                        let name = obj.name_any();
                        let namespace = match obj.namespace() {
                            Some(ns) => ns,
                            None => "unknown".to_string(),
                        };
                        let kind = match obj.types {
                            Some(ref tp) => tp.kind.clone(),
                            None => "none".to_owned(),
                        };

                        println!(">> kind: {0:<20} name: {1:<40} ns: {2:}",
                            kind,
                            name,
                            namespace);
                    }
                    println!("\n");
                },
                WatchCommand::None => {
                    tracing::debug!("No OPS");
                },
            }
        }
    });
    
    // print in regular intervals the contents of the cache
    tokio::spawn(async move {
        loop {
            tokio::select!{
                _ = async {
                    ipoll.tick().await;
                }=>{
                    let _res = tx_poll.send(WatchEvent { 
                            command: WatchCommand::PrintAll,
                            ..WatchEvent::default()
                        }).await;
                }           
            }
        }
    });

    // purge the cache in regular intervals
    tokio::spawn(async move {
        loop {
            tokio::select!{
                _ = async {
                    ipurge.tick().await;
                }=>{
                    let _res = tx_purge.send(WatchEvent { 
                        command: WatchCommand::Purge,
                        ..WatchEvent::default()
                    }).await;
                }           
            }
        }
    });

    Ok(())
}

// Process the watched DynamicObject before caching
async fn process_dynobj(obj: DynamicObject, 
    res_url: String,
    tx_api: Sender<String>,
    rx_type: &mut Receiver<Option<TypeMeta>>) -> Result<DynamicObject> {

    let mut obj_with_type: DynamicObject = if let Some(_type_meta) = &obj.types {
                        dbg!(&obj.types);
                        DynamicObject {
                            types: obj.types,
                            metadata: obj.metadata,
                            data: obj.data,
                        }
                    }else{ 
                        let types = match tx_api.send(res_url.clone()).await{
                            Ok(_) => { 
                                if let Some(res) = rx_type.recv().await {
                                    if let Some(tm) = res {
                                        Some(tm)
                                    }else{
                                        None
                                    }
                                }else{
                                    None
                                }
                            },        
                            Err(why) => {
                                tracing::error!("Failed extracting k8s type from URL: {:?}", why);
                                return Err(why.into())
                            },
                        };

                        // Some(tm.kind.clone())
                        DynamicObject {
                            types,
                            metadata: obj.metadata,
                            data: obj.data,
                        }
                    };
    obj_with_type.
            annotations_mut().
            remove("kubectl.kubernetes.io/last-applied-configuration");

    obj_with_type.managed_fields_mut().clear();
    
     Ok(obj_with_type)
}

// print to stdout the contents of the cache
async fn _print_cache_db(cache: &Db) {
    println!("\n>> Printing Cache DynamicObjects");
    let db = cache.lock().unwrap();
    for (_, obj) in db.iter() {
        let name = obj.name_any();
        let namespace = match obj.namespace() {
            Some(ns) => ns,
            None => "unknown".to_string(),
        };
        let kind = match obj.types {
            Some(ref tp) => tp.kind.clone(),
            None => "none".to_owned(),
        };

        println!(">> kind: {0:<20} name: {1:<40} ns: {2:}",
            kind,
            name,
            namespace);
    }
    println!("");
}

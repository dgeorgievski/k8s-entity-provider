use crate::ax_kube::{
    client, 
    discovery, 
    watch_event::WatchCommand, 
    WatchEvent};

use anyhow::Result;
use futures::{stream, StreamExt, TryStreamExt};
use kube::{
    core::ApiResource,
    api::{Api, DynamicObject}, 
    runtime::watcher, 
    ResourceExt};
// use kube::ResourceExt;
use tokio::sync::mpsc::{channel, Receiver, Sender};
// use tracing::field;
use crate::configuration::Settings;
#[derive(Debug)]
enum SelectedEvents {
    Applied(watcher::Event<DynamicObject>),
    Deleted(watcher::Event<DynamicObject>),
    Restarted(watcher::Event<DynamicObject>),
}

pub struct EventsChannels {
    pub rx: Receiver<WatchEvent>,
    pub tx: Sender<WatchEvent>,
}

// watch - Starts threads to track configured resources, and Senders and a Receiver channels 
//         for communicating results as WatchEvents
// pub async fn watch(conf: &Settings, k8s_version: String) -> Result<Receiver<WatchEvent>> {
pub async fn watch(conf: &Settings, k8s_version: String) -> Result<EventsChannels> {
    let (tx, rx): (Sender<WatchEvent>, Receiver<WatchEvent>) = channel(32);

    let cli = match client::client(conf.kube.use_tls).await {
        Err(why) => {
            tracing::error!("k8s Client failed {:?}", why);
            return Err(why.into())
        }
        Ok(cli) => {
            tracing::info!("Succesfully connected to k8s");
            cli
        }
    };

    let discovery = discovery::new(&cli).await?;
    // Common discovery, parameters, and api configuration for a single resource
    let api_res = discovery::resolve_api_resources( 
                        &discovery, 
                        &conf.kube.resources);

    for (ares, caps) in api_res {
        println!("\n\n ApiRes {:?}\n CAP: {:?}", ares, caps); 

        let dyn_apis = discovery::dynamic_api(
                                            ares, 
                                            caps,
                                            cli.clone(), 
                                            &conf.kube.resources);

        for apisel in dyn_apis { 
            let k8s_ver = k8s_version.clone();
            let tx2 = tx.clone();
            let resource_url: String = apisel.api_dyn.resource_url().to_owned();

            // start watching API Resource in a dedicated thread
            tokio::spawn(async move {
                let mut wc = watcher::Config::default();
                if let Some(sel) = apisel.field_selectors {
                    if sel.len() > 0 {
                        wc.field_selector = Some(sel.join(","));
                        println!("Added field selectors {:?} url: {}", 
                            wc.field_selector, 
                            resource_url);
                    }
                }

                if let Some(sel) = apisel.label_selectors {
                    if sel.len() > 0 {
                        wc.label_selector = Some(sel.join(","));
                        println!("Added label selectors {:?} url: {}", 
                            wc.label_selector, 
                            resource_url);
                    }
                }

                // applied_objects().
                let stream_applied = watcher(apisel.api_dyn.clone(), 
                                                wc.clone()).
                                                map_ok(SelectedEvents::Applied);

                let stream_deleted = watcher(apisel.api_dyn.clone(), 
                                                wc.clone()).
                                                    map_ok(SelectedEvents::Deleted);

                let stream_restarted = watcher(apisel.api_dyn.clone(), 
                                                    wc.clone()).
                                                        map_ok(SelectedEvents::Restarted);
    
                let mut stream_all =  stream::select_all(vec![
                    stream_applied.boxed(),
                    stream_deleted.boxed(),
                    stream_restarted.boxed(),
                ]);

                loop {
                    let cmds: Vec<WatchCommand> = match stream_all.try_next().await {
                            Ok(sel_event) => {
                                // TODO test new watch::Event types
                                match sel_event {
                                    Some(SelectedEvents::Applied(watcher::Event::Apply(o))) => {
                                        println!(" >> SEL add {:?}", o.name_any());
                                        dbg!(&o.types);
                                        vec![WatchCommand::Add(o)]
                                    },
                                    Some(SelectedEvents::Deleted(watcher::Event::Delete(o))) => {
                                        println!(" >> SEL del {:?}", o.name_any());
                                        vec![WatchCommand::Delete(o)]
                                    },
                                    Some(SelectedEvents::Restarted(watcher::Event::InitApply(o))) => {
                                        let mut cmds: Vec<WatchCommand> = Vec::new();
                                        // for o in objs.iter() {
                                        //     println!(" >> SEL res {:?} types: {:?}", &o.name_any(), &o.types);
                                        //     cmds.push(WatchCommand::Add(o.clone()));
                                        // }
                                        println!(" >> SEL res {:?} types: {:?}", &o.name_any(), &o.types);
                                        cmds.push(WatchCommand::Add(o.clone()));
                                        cmds
                                    },
                                    _ => {
                                        continue;
                                    }
                                }
                            },
                            Err(why) => {
                                tracing::error!("failed to get stream_all response: {:?}", why); 
                                continue;
                            },
                        };

                    for cmd in cmds.iter() {
                        let we = WatchEvent{
                            k8s_version: k8s_ver.clone(),
                            resource_url: resource_url.clone(),
                            event_type: apisel.event_type.clone(),
                            command: cmd.clone(),
                        };
                        tx2.send(we).await.unwrap();
                    };
                }
            });
        }
    }
    return Ok(EventsChannels{
        rx,
        tx: tx.clone(),
    });
}

// Check if k8s resources is still ready in the cluster.
pub async fn check_objects(objs: Vec<DynamicObject>, conf: &Settings) -> Result<Vec<DynamicObject>> {
    let cli = match client::client(conf.kube.use_tls).await {
            Err(why) => {
                tracing::error!("k8s Client failed {:?}", why);
                return Err(why.into())
            }
            Ok(cli) => {
                tracing::info!("Succesfully connected to k8s");
                cli
            }
        };

    let mut missing: Vec<DynamicObject> = Vec::new();

    for o in objs.iter() {
        let name = o.name_any();
        let namespace = match &o.metadata.namespace {
            Some(ns) => ns.clone(),
            None => {
                tracing::error!("check_obj missing namespace for {:?}", 
                        o.name_any());
                continue;
            }
        };

        let tp = match &o.types {
            Some(tp) => tp.clone(),
            None => {
                tracing::error!("check_obj missing TypeMeta for {:?}/{:?}", 
                    namespace.clone(),
                    o.name_any());
                continue;
            },
        };

        let gr_ver: Vec<&str> = tp.api_version.split("/").collect();
        let (group, ver) = match gr_ver.len() {
            1 => {
                (String::from(""), gr_ver[0].to_owned())
            },
            2 => {
                (gr_ver[0].to_owned(), gr_ver[1].to_owned())
            },
            _ => {
                tracing::error!("check_obj incorrect apiVersion for {:?}/{:?}/{:?}", 
                namespace.clone(),
                o.name_any(),
                tp.api_version);
                continue;
            }
        };

        let ar = ApiResource { 
            group: group, 
            version: ver, 
            api_version: tp.api_version, 
            kind: tp.kind.clone(), 
            plural: format!("{:}s", tp.kind.to_lowercase()),
        };

        let api: Api<DynamicObject> = Api::namespaced_with(
            cli.clone(), 
            namespace.as_str(),
            &ar);

        match api.get_opt(name.as_str()).await {
            Ok(k8s_obj) => {
                match k8s_obj {
                    Some(_dynobj) => {
                        // println!(" >> found {:?}", dynobj.name_any());
                        continue;
                    },
                    None => {
                        // println!(" >> missing {:?}/{:}", namespace.clone(), name.clone());
                        missing.push(o.clone());
                    },
                }
            },
            Err(why) => {
                tracing::error!("Failed k8s get {:?}", why);
            }
        };
    }

    Ok(missing)
}
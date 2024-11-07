use serde::{Deserialize, Serialize};
use anyhow::{anyhow, Result};
use http;
use crate::configuration::Settings;
use crate::ax_kube::client;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ServerVersion {
    pub major: String,
    pub minor: String,
    pub platform: String,
}

pub async fn get_k8s_version(conf: &Settings) -> Result<ServerVersion>{
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

    // make a direct http request to k8s API
    match http::Request::get("/version").body(Default::default()) {
        Ok(req)=> {
            match cli.request::<serde_json::Value>(req).await{
                Ok(resp) => {
                    match serde_json::from_value::<ServerVersion>(resp.to_owned()) {
                        Ok(sv) => {
                            dbg!(sv.clone());
                            return Ok(sv)
                        },
                        Err(why) => {
                            let errm = format!("failed json ServerVersion conversion {:?}", why);
                            tracing::error!(errm);    
                            return Err(anyhow!(errm));
                        }
                    };

                },
                Err(why) => {
                    let errm = format!("failed json ServerVersion conversion {:?}", why);
                    tracing::error!(errm);    
                    return Err(anyhow!(errm));
                }  
            };
        },
        Err(why) => {
            let errm = format!("failed json ServerVersion conversion {:?}", why);
            tracing::error!(errm);    
            return Err(anyhow!(errm));
        },
    };

}
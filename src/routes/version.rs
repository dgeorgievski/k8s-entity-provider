use actix_web::{get, web, Responder, Result};
use std::str::FromStr;
use semver::Version;
use crate::configuration::Settings;

#[get("/version")]
pub async fn bs_provider_version(data: web::Data<Settings>) -> Result<impl Responder> {
    Ok(web::Json(get_version(data.display.clone())))
}

#[derive(serde::Serialize, Debug, Clone)]
pub struct HCVersion {
    pub app: String,
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: String,
}

pub fn get_version(app_name: String) -> HCVersion {
    let ver = env!("CARGO_PKG_VERSION");

    // todo - get app name from config
    match Version::from_str(ver) {
        Ok(v) => {
            HCVersion {
                app: app_name, 
                major: v.major,
                minor: v.minor,
                patch: v.patch,
                pre: v.pre.to_string(),
            }
        },
        Err(why) => {
            HCVersion {
                app: app_name,
                major: 0,
                minor: 0,
                patch: 0,
                pre: why.to_string(),
            }
        }
    }
} 
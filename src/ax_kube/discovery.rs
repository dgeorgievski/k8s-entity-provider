use crate::ax_kube::apigroup::{AllResource, ApiCapabilities, ApiGroup, ApiResource};
use crate::configuration as config;
use std::collections::HashMap;

use anyhow::Result;

use kube::{
    api::{Api, DynamicObject},
    discovery::Scope,
    Client,
};

#[allow(dead_code)]
enum DiscoveryMode {
    /// Only allow explicitly listed apigroups
    Allow(Vec<String>),
    /// Allow all apigroups except the ones listed
    Block(Vec<String>),
}

impl DiscoveryMode {
    fn is_queryable(&self, group: &String) -> bool {
        match &self {
            Self::Allow(allowed) => allowed.contains(group),
            Self::Block(blocked) => !blocked.contains(group),
        }
    }
}

pub struct Discovery {
    client: Client,
    groups: HashMap<String, ApiGroup>,
    mode: DiscoveryMode,
}

impl Discovery {
    /// Construct a caching api discovery client
    #[must_use]
    pub fn new(client: Client) -> Self {
        let groups = HashMap::new();
        let mode = DiscoveryMode::Block(vec![]);
        Self {
            client,
            groups,
            mode,
        }
    }

    /// Returns iterator over all served groups
    pub fn groups(&self) -> impl Iterator<Item = &ApiGroup> {
        self.groups.values()
    }

    pub async fn run(mut self) -> Result<Self> {
        self.groups.clear();
        let api_groups = self.client.list_api_groups().await?;
        
        // query regular groups + crds under /apis
        for g in api_groups.groups {
            let key = g.name.clone();
            if self.mode.is_queryable(&key) {
                let apigroup = ApiGroup::query_apis(&self.client, g).await?;
                self.groups.insert(key, apigroup);
            }
        }
        
        // query core versions under /api
        let corekey = ApiGroup::CORE_GROUP.to_string();
        if self.mode.is_queryable(&corekey) {
            let coreapis = self.client.list_core_api_versions().await?;
            let apigroup = ApiGroup::query_core(&self.client, coreapis).await?;
            self.groups.insert(corekey, apigroup);
        }
        Ok(self)
    }
}

pub fn resolve_api_resources(
    discovery: &Discovery,
    resources: &Vec<config::Resource>,
)-> Vec<(ApiResource, ApiCapabilities)> {
    let target_res: HashMap<String, Option<Vec<String>>> = resources
                    .iter()
                    .map(|res| {
                        (res.name.to_lowercase(), res.api_groups.clone())
                    })
                    .collect();


    // iterate through groups to find matching kind/plural names at recommended versions
    // and then take the minimal match by group.name (equivalent to sorting groups by group.name).
    // this is equivalent to kubectl's api group preference
    discovery
        .groups()
        .flat_map(|group| {
            group
                .recommended_resources()
                .into_iter()
                .map(move |res| (group, res))
        })
        .filter(|(_, (res, _))| {  
            let is_found = if let Some(api_key) = target_res.get(&res.kind.to_lowercase()) {
                if let Some(api_groups) = api_key {
                    let mut is_apg = false;
                    for apg in api_groups {
                        if apg == &res.group {
                            is_apg = true;
                            break;
                        }
                    }
                    is_apg
                }else{
                    true
                }
            }else{
                // plural
                if let Some(api_key) = target_res.get(&res.plural.to_lowercase()) {
                    if let Some(api_groups) = api_key {
                        let mut is_apg = false;
                        for apg in api_groups {
                            if apg == &res.group {
                                is_apg = true;
                                break;
                            }
                        }
                        is_apg
                    }else{
                        true
                    }
                }else{
                    false
                }
            };
            // for r in &res_names {
            //     if r.eq_ignore_ascii_case(&res.kind) || 
            //         r.eq_ignore_ascii_case(&res.plural) {
            //             is_found = true;
            //             break;    
            //         }
            // }

            is_found
        })
        .map(|(_, res)| res)
        .collect()
}

#[derive(Debug, Clone)]
pub struct ApiWithSelectors {
    pub label_selectors: Option<Vec<String>>,
    pub field_selectors: Option<Vec<String>>,
    pub event_type: String,
    pub api_dyn: Api<DynamicObject>,
}

pub fn dynamic_api(
    ar: ApiResource,
    caps: ApiCapabilities,
    client: Client,
    resources: &Vec<config::Resource>,
) -> Vec<ApiWithSelectors> {

    let mut dyn_apis: Vec<ApiWithSelectors> = vec![];
    // let key_kind = resources.get(&ar.kind);
    // if key_kind == None { return dyn_apis }
    let ar_kind = ar.kind.clone().to_ascii_lowercase();
    let ar_plural = ar.plural.clone().to_ascii_lowercase();

    for res in resources {
        let r_kind = res.name.to_ascii_lowercase();
        // println!(" > checking {:?} == {:?} == {:?}", r_kind, ar_kind, ar_plural);
        if r_kind != ar_kind && r_kind != ar_plural{
            continue;
        }

        if caps.scope == Scope::Cluster {
            dyn_apis.push(ApiWithSelectors{
                event_type: res.event_type.clone(),
                label_selectors: Some(res.label_selectors.clone()),
                field_selectors: Some(res.field_selectors.clone()),
                api_dyn: Api::all_with(client.clone(), 
                                        &ar.clone().to_kube_ar()),
            });
        } else if res.namespaces.len() > 0 {
            for ns in &res.namespaces {
                    dyn_apis.push(ApiWithSelectors{
                        event_type: res.event_type.clone(),
                        label_selectors: Some(res.label_selectors.clone()),
                        field_selectors: Some(res.field_selectors.clone()),
                        api_dyn: Api::namespaced_with(client.clone(), 
                                                        &ns, 
                                                        &ar.clone().to_kube_ar())}
                    );
            }
        } else if res.namespaces.len() == 0 {
            dyn_apis.push(ApiWithSelectors{
                event_type: res.event_type.clone(),
                label_selectors: Some(res.label_selectors.clone()),
                field_selectors: Some(res.field_selectors.clone()),
                api_dyn: Api::all_with(client.clone(), 
                                        &ar.clone().to_kube_ar())}
            );
        } else {
            tracing::error!("No resources provided");
            // dyn_apis.push(Api::default_namespaced_with(client, &ar.to_kube_ar()));
        };    
    };

    dyn_apis

}

pub async fn new(cli: &Client) -> Result<Discovery> {
    let discovery = Discovery::new(cli.clone()).run().await?;
    Ok(discovery)
}

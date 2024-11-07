use std::{any::Any, collections::HashMap};
use serde::ser::{
    Serialize, 
    Serializer,
    SerializeStruct,
};
use serde_json::Value;
use kube::{core::DynamicObject, ResourceExt};
use anyhow::Result;
use std::fmt;
use crate::configuration::{BackstageSettings, Settings};
// use serde_aux::field_attributes::deserialize_number_from_string;
// use std::convert::{TryFrom, TryInto};

const BACKSTAGE_DEFAULT_OWNER: &str = "platform"; 
const BACKSTAGE_ENTITY_API_VERSION: &str = "backstage.io/v1alpha1";
const BACKSTAGE_ENTITY_RESOURCE: &str = "Resource";
const BACKSTAGE_ENTITY_COMPONENT: &str = "Component";
const BACKSTAGE_ENTITY_USER: &str = "User";
const BACKSTAGE_ENTITY_GROUP: &str = "Group";
const BACKSTAGE_ENTITY_DOMAIN: &str = "Domain";
const BACKSTAGE_ENTITY_SYSTEM: &str = "System";
const BACKSTAGE_ENTITY_NONE: &str = "none";
const BACKSTAGE_ANN_LABEL_SELECTOR: &str = "backstage.io/kubernetes-label-selector";
const BACKSTAGE_ANN_NAMESPACE: &str = "backstage.io/kubernetes-namespace";
const AXYOMCORE_ANN_CLUSTER: &str = "acme.com/kubernetes-cluster";
const REDIS_LABEL_CLUSTER: &str = "postgres.acme.com/name";
const REDIS_LABEL_SHARD: &str = "shard.acme.com/name";
const REDIS_LABEL_K8S_NAME: &str = "app.kubernetes.io/component";

// custom annotations to convey state
const AXYOM_ANN_REDIS_STATUS: &str = "backstage.acme.com/postgres-status";

/*
See https://backstage.io/docs/features/software-catalog/descriptor-format
*/
#[derive(serde::Serialize, serde::Deserialize, Default, Debug, Clone)]
pub struct Metadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub links: Option<Vec<Link>>,
}

impl Metadata {
    pub fn default() ->Self {
        Self {
            // default namespace is the only option currently
            namespace: Some(String::from("default")),
            ..Default::default()
        }
    }

    pub fn new(name: String) -> Self { 
        Self {
            name,
            namespace: Some("default".to_owned()),
            ..Default::default()
        }
    }

    pub fn from_annotations(bsc: &BackstageSettings, name: String) -> Self {
        match &bsc.annotations {
            Some(anns) => Self {
                    name,
                    namespace: Some("default".to_owned()),
                    annotations: Some(anns.clone()),
                    ..Default::default()
                },
            None => Metadata::new(name),
        }
    }

    // add global settings to those configured for the static entity like Group
    pub fn from_static_config(bsc: BackstageSettings, md: Metadata) -> Self {
            // glbal annotations
        let anns: HashMap<String, String> = match bsc.annotations {
            Some(anns) => anns,
            None => HashMap::new(),
        };

        // entity annotations
        match md.annotations {
            Some(ref en_anns) => {
                let mut anns2 = anns.clone();
                for (a, v) in en_anns.iter() {
                    anns2.insert(a.clone(), v.clone());
                }

                Self { 
                    namespace: Some("default".to_owned()),
                    annotations: Some(anns),
                    ..md
                }},
            None => Self { 
                namespace: Some("default".to_owned()),
                annotations: Some(anns),
                ..md
            }
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Link {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
pub struct Component {
    #[serde(rename(serialize = "apiVersion", deserialize = "apiVersion"))]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: ComponentSpec,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
pub struct ComponentSpec {
    pub r#type: String,
    pub lifecycle: String,
    pub owner: String,
    pub system: Option<String>,
    #[serde(rename(serialize = "subcomponentOf", deserialize = "subcomponentOf"))]
    pub subcomponent_of: Option<String>,
    #[serde(rename(serialize = "providesApis", deserialize = "providesApis"))]
    pub provides_apis: Option<Vec<String>>,
    #[serde(rename(serialize = "consumesApis", deserialize = "consumesApis"))]
    pub consumes_apis: Option<Vec<String>>,
    #[serde(rename(serialize = "dependsOn", deserialize = "dependsOn"))]
    pub depends_on: Option<Vec<String>>,
    #[serde(rename(serialize = "dependencyOf", deserialize = "dependencyOf"))]
    pub dependency_of: Option<Vec<String>>
}

impl Component {
    pub fn default() -> Self {
        Self {
            api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
            kind: BACKSTAGE_ENTITY_COMPONENT.to_string(),
            metadata: Metadata::default(),
            spec: ComponentSpec{
                r#type: String::from("service"),
                lifecycle: String::from("experimental"),
                owner: String::from("platform"),
                ..Default::default()
            }
        }
    }

    pub fn from_deployment(bsc: BackstageSettings, obj: &DynamicObject) -> Result<Self, EntityError> {
        // validations
        //check if StatefulSet
        if let Some(ref tp) = obj.types {
            if tp.kind.to_lowercase() != "deployment" {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_COMPONENT.to_owned(),
                    name: obj.name_any().clone(),
                    message: "Resource is not a k8s Deployment".to_owned(),
                });
            }
        }else{
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_COMPONENT.to_owned(),
                name: obj.name_any().clone(),
                message: "Resource lacks TypeMeta data".to_owned(),
            });
        }

        let mut spec_type = String::from("deployment"); // todo add validations and enums
        let mut m = Metadata::from_annotations(&bsc,
            obj.name_any().clone());

            let mut anns:HashMap<String, String> = match m.annotations {
                Some(ref a) => a.clone(),
                None => HashMap::new()
            };          
            let mut lbls: HashMap<String, String> = HashMap::new();
            let ns = match obj.metadata.namespace {
                Some(ref namespace) => namespace,
                None => &String::from("default"),     
            };
            
            // todo improve validations
            if m.name == "" {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                    name: obj.name_any().clone(),
                    message: "Resource lacks lacks Metadata".to_owned(),
                });
            }
    
            for (label, val) in obj.labels() {
                // copy k8s labels
                lbls.insert(label.to_string(), val.to_string());
    
                // add annotations that assoiate Entities to k8s Resources
                if label.eq(REDIS_LABEL_SHARD) {
                    // backstage.io/kubernetes-label-selector: shard.acme.com/name: tenant-smf-smfpostgres-0
                    anns.insert(BACKSTAGE_ANN_LABEL_SELECTOR.to_string(), 
                        format!("{0:}={1:}", 
                        label,
                        val));
    
                    // backstage.io/kubernetes-namespace: tenant-smf
                    anns.insert(BACKSTAGE_ANN_NAMESPACE.to_string(), 
                                ns.to_string());
                }
    
                // check if sts is a Redis cluster
                if label.eq(REDIS_LABEL_K8S_NAME) && val.eq("postgres-cluster") {
                    spec_type = String::from("postgres-cluster")
                }
            }
    
            if !lbls.is_empty() {
                m.labels = Some(lbls);
            }
    
            if let Some(ref bs_anns) = m.annotations {
                for (a, v) in bs_anns.iter(){
                    anns.insert(a.clone(), v.clone());
                }
            }
            m.annotations = Some(anns);
            
            Ok(Self {  
                api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
                kind: BACKSTAGE_ENTITY_COMPONENT.to_string(),
                metadata: m,
                spec: ComponentSpec {
                    r#type: spec_type,
                    owner: String::from(BACKSTAGE_DEFAULT_OWNER.to_owned()),
                    ..Default::default()
                }
            })
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct Resource {
    #[serde(rename(serialize = "apiVersion", deserialize = "apiVersion"))]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: ResourceSpec,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct ResourceSpec {
    pub r#type: String,
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename(serialize = "dependsOn"))]
    pub depends_on: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename(serialize = "dependencyOf"))]
    pub dependency_of: Option<Vec<String>>,
}

impl Resource {
    pub fn default() -> Self {
        Self {
            api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
            kind: BACKSTAGE_ENTITY_RESOURCE.to_string(),
            metadata: Metadata::default(),
            spec: ResourceSpec{
                r#type: String::from(BACKSTAGE_ENTITY_NONE),
                owner: String::from("platform"),
                ..Default::default()
            }
        }
    }
 
    // Converts k8s StatefulSet to Backstage Resource
    pub fn postgres_shard_from_statefulset(config: &Settings, 
        obj: &DynamicObject) -> Result<Self, EntityError> {
        // validations
        //check if StatefulSet
        let bsc = &config.backstage;
        if let Some(ref tp) = obj.types {
            if tp.kind.to_lowercase() != "statefulset" {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                    name: obj.name_any().clone(),
                    message: "Resource is not a k8s StatefulSet".to_owned(),
                });
            }
        }else{
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                name: obj.name_any().clone(),
                message: "Resource lacks TypeMeta data".to_owned(),
            });
        }

        // links Redis cluster to its shards - entity:namespace
        let en_ref_prefix = String::from("resource:default");
        let mut spec_type = String::from("statefulset"); // todo add validations and enums
        let mut m = Metadata::from_annotations(bsc,
                                            obj.name_any().clone());

        let mut anns:HashMap<String, String> = match m.annotations {
            Some(ref a) => a.clone(),
            None => HashMap::new()
        };          
        let mut lbls: HashMap<String, String> = HashMap::new();
        let ns = match obj.metadata.namespace {
            Some(ref namespace) => namespace,
            None => &String::from("default"),     
        };
        
        // todo improve validations
        if m.name.len() == 0 {
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                name: obj.name_any().clone(),
                message: "Resource lacks Metadata name".to_owned(),
            });
        }

        // let mut postgres_system: Option<String> = None;
        let mut shard_dependency_of: Option<Vec<String>> = None;
        for (label, val) in obj.labels() {
            // copy k8s labels
            lbls.insert(label.to_string(), val.to_string());

            if label.eq(REDIS_LABEL_CLUSTER) {
                // let cl_lower = val.to_lowercase().clone();
                // postgres_system = if cl_lower.contains("upf") {
                //      Some(format!("upf-postgres-{}", config.cluster.clone()))
                // }else if cl_lower.contains("smf") {
                //     Some(format!("smf-postgres-{}", config.cluster.clone()))
                // }else{
                //     None
                // };
                shard_dependency_of = Some(vec![
                    format!("{}/{}", 
                            en_ref_prefix.clone(), 
                            val.clone())
                    ]);
            }

            // add annotations that assoiate Entities to k8s Resources
            if label.eq(REDIS_LABEL_SHARD) {
                // backstage.io/kubernetes-label-selector: shard.acme.com/name: tenant-smf-smfpostgres-0
                anns.insert(BACKSTAGE_ANN_LABEL_SELECTOR.to_string(), 
                    format!("{0:}={1:}", 
                    label,
                    val));

                // backstage.io/kubernetes-namespace: tenant-smf
                anns.insert(BACKSTAGE_ANN_NAMESPACE.to_string(), 
                            ns.to_string());
            }

            // check if sts is a Redis cluster
            if label.eq(REDIS_LABEL_K8S_NAME) && val.eq("postgres-cluster") {
                spec_type = String::from("postgres-cluster-shard")
            }
        }

        // add k8s cluster name
        anns.insert(AXYOMCORE_ANN_CLUSTER.into(), config.cluster.clone());

        let status_anns = match spec_type.as_str() {
            "postgres-cluster-shard" => {
                Self::postgres_status_from_statefulset(&obj)
            },
            _ => None
        };

        if let Some(stans) = status_anns {
            anns.insert(AXYOM_ANN_REDIS_STATUS.to_string(), stans);
        }

        if !lbls.is_empty() {
            m.labels = Some(lbls);
        }

        if let Some(ref bs_anns) = m.annotations {
            for (a, v) in bs_anns.iter(){
                anns.insert(a.clone(), v.clone());
            }
        }
        m.annotations = Some(anns);
        
        Ok(Self {  
            api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
            kind: BACKSTAGE_ENTITY_RESOURCE.to_string(),
            metadata: m,
            spec: ResourceSpec {
                r#type: spec_type,
                owner: BACKSTAGE_DEFAULT_OWNER.to_owned(),
                // system: postgres_system,
                dependency_of: shard_dependency_of,
                ..Default::default()
            }
        })
    }

    // Create Redis cluster Resource from Redis Shard Resource
    pub fn postgres_cluster_from_shard(config: &Settings, postgres: Resource) -> Result<Self, EntityError> {
        // links Redis cluster to its shards - entity:namespace
        let en_ref_prefix = String::from("resource:default");
        let mut m = postgres.metadata;
        let mut depends_on: Option<Vec<String>> = None;
        let mut cluster_labels = m.labels.clone().unwrap();
        cluster_labels.remove(REDIS_LABEL_CLUSTER);

        let mut postgres_system: Option<String> = None;
        match &m.labels {
            Some(labels) => {
                if let Some(cluster) = labels.get(REDIS_LABEL_CLUSTER) {
                    m.name = cluster.to_string();
                    let cl_lower = cluster.to_lowercase().clone();
                    postgres_system = if cl_lower.contains("upf") {
                        Some(format!("upf-postgres-{}", config.cluster.clone()))
                    }else if cl_lower.contains("smf") {
                        Some(format!("smf-postgres-{}", config.cluster.clone()))
                    }else{
                        None
                    };
                }
           
                if let Some(shard) = labels.get(REDIS_LABEL_SHARD) {
                    depends_on = Some(vec![
                        format!("{}/{}", en_ref_prefix.clone(), 
                            shard.clone())
                        ]);
                }
            },
            None => {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                    name: m.name.clone(),
                    message: "Resource lacks postgres labels".to_owned(),
                })
            },
        }

        m.labels = Some(cluster_labels);
        Ok(Self {  
            api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
            kind: BACKSTAGE_ENTITY_RESOURCE.to_string(),
            metadata: m,
            spec: ResourceSpec {
                r#type: "postgres-cluster".to_owned(),
                owner: String::from(BACKSTAGE_DEFAULT_OWNER.to_owned()),
                system: postgres_system,
                depends_on,
                ..Default::default()
            }
        })
    }

    // extract status of a Redis StatefulSet cluster
    fn postgres_status_from_statefulset(obj: &DynamicObject) -> Option<String> {
        match obj.data.get("status") {
            Some(Value::Object(status)) => {
                match serde_json::to_string(status){
                    Ok(st) => Some(st),
                    Err(why) => {
                        tracing::error!("failed to extract postgres status {}", why);
                        None
                    }
                }
                
            },
            Some(_) => {
                println!("wrong status value type for sts {}", obj.name_any());
                None
            },
            None => None,
        }
    }

    // Create Redis Cluster Node Resource fro a k8s Pod 
    pub fn postgres_node_from_pod(config: &Settings, 
        obj: &DynamicObject) -> Result<Self, EntityError> {
        // validations
        let bsc = &config.backstage;
        if let Some(ref tp) = obj.types {
            if tp.kind.to_lowercase() != "pod" {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                    name: obj.name_any().clone(),
                    message: "Resource is not a k8s Pod".to_owned(),
                });
            }
        }else{
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                name: obj.name_any().clone(),
                message: "Resource lacks TypeMeta data".to_owned(),
            });
        }

        let en_ref_prefix = String::from("resource:default");
        let m = Metadata::from_annotations(bsc,
            obj.name_any().clone());
        if m.name.len() == 0 {
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                name: obj.name_any().clone(),
                message: "Resource lacks Metadata name".to_owned(),
            });
        }   
        let mut dependency_of: Option<Vec<String>> = None;
        
        match &obj.metadata.labels {
            Some(labels) => {
                if let Some(shard) = labels.get(REDIS_LABEL_SHARD) {
                    dependency_of = Some(vec![
                        format!("{}/{}", en_ref_prefix.clone(), 
                            shard.clone())
                        ]);
                }
            },
            None => {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_RESOURCE.to_owned(),
                    name: m.name.clone(),
                    message: "Resource lacks postgres labels".to_owned(),
                })
            },
        }

        Ok(Self {
            api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
            kind: BACKSTAGE_ENTITY_RESOURCE.to_string(),
            metadata: m,
            spec: ResourceSpec {
                r#type: "postgres-cluster-node".to_owned(),
                owner: BACKSTAGE_DEFAULT_OWNER.to_owned(),
                dependency_of,
                ..Default::default()
            }
        })
    }
}

// impl fmt::Display for Resource {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {

//         write!(f, "Circle of radius {}", self.radius)
//     }
// }

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct Group {
    #[serde(rename(serialize = "apiVersion", deserialize = "apiVersion"))]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: GroupSpec,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct GroupSpec {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub children: Vec<String>,
}

impl Group {

    /*
        Instantiate a list of Group entities from the app config.
        Return an empty list if no config is provided.
     */
    pub fn groups_from_config(bsc: BackstageSettings) -> Vec<Self>{
        let mut res:Vec<Self> = Vec::new();

        for g in bsc.groups.iter() {
            // let parent = match g.spec.parent.clone() {
            //     Some(p) => p,
            //     None => "".to_owned(),
            // };

            // let children: Vec<String> = match &g.spec.children {
            //     Some(c) => c.to_vec(),
            //     None => Vec::new(),
            // };

            let m = Metadata::from_static_config(bsc.clone(),
                g.metadata.clone());

            res.push(
                Self { 
                    api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(), 
                    kind: BACKSTAGE_ENTITY_GROUP.to_string(), 
                    metadata: m, 
                    spec: g.spec.clone()});
        }
        
        res
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct User {
    #[serde(rename(serialize = "apiVersion", deserialize = "apiVersion"))]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: UserSpec,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct UserSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<HashMap<String, String>>,
    #[serde(rename(serialize = "memberOf"))]
    pub member_of: Vec<String>,
}

impl User {
    /*
        Instantiate a list of User entities from the app config.
        Return an empty list if no config is provided.
     */
    pub fn users_from_config(bsc: BackstageSettings) -> Vec<Self>{
        let mut res:Vec<Self> = Vec::new();
        
        for u in bsc.users.iter() {
            // let member_of: Vec<String> = match &u.spec.member_of {
            //     Some(m) => m.to_vec(),
            //     None => Vec::new(),
            // };

            let m = Metadata::from_static_config(bsc.clone(),
                u.metadata.clone());

            // let mut mt = u.metadata.clone();
            res.push(
                Self { 
                    api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(), 
                    kind: BACKSTAGE_ENTITY_USER.to_string(), 
                    metadata: m, 
                    spec: u.spec.clone() }
            );
        }
        
        res
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct Domain {
    #[serde(rename(serialize = "apiVersion", deserialize = "apiVersion"))]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: DomainSpec,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct DomainSpec {
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none", 
        rename(serialize = "subdomainOf"))]
    pub subdomain_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

impl Domain {
    pub fn domains_from_config(bsc: BackstageSettings) -> Vec<Self> {
        let mut domains: Vec<Self> = Vec::new();

        if let Some(ref conf_domains) = bsc.domains {
            for d in conf_domains.iter() {
                let m = Metadata::from_static_config(bsc.clone(),
                    d.metadata.clone());
                
                domains.push(
                    Self { 
                        api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(), 
                        kind: BACKSTAGE_ENTITY_DOMAIN.to_string(), 
                        metadata: m, 
                        spec: d.spec.clone() }
                );
            }
        }
        domains
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct System {
    #[serde(rename(serialize = "apiVersion", deserialize = "apiVersion"))]
    pub api_version: String,
    pub kind: String,
    pub metadata: Metadata,
    pub spec: SystemSpec,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct SystemSpec {
    pub owner: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
}

impl System {
    // Creates a System from k8s Redis StatefulSet
    pub fn from_stateful_set(config: &Settings, obj: &DynamicObject) -> Result<Self, EntityError> {
        if let Some(ref tp) = obj.types {
            if tp.kind.to_lowercase() != "statefulset" {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_SYSTEM.to_owned(),
                    name: obj.name_any().clone(),
                    message: "Resource is not a k8s StatefulSet".to_owned(),
                });
            }
        }else{
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_SYSTEM.to_owned(),
                name: obj.name_any().clone(),
                message: "Resource lacks TypeMeta data".to_owned(),
            });
        }

        let name = match obj.labels().get(REDIS_LABEL_CLUSTER) {
            Some(cluster_name) => cluster_name,
            None => {
                return Err(EntityError{ 
                    kind: BACKSTAGE_ENTITY_SYSTEM.to_owned(),
                    name: obj.name_any().clone(),
                    message: "Statefulset lacks postgres cluster label".to_owned(),
                });
            }
        };

        let nm_lcase = name.to_lowercase();
        let postgres_system = if nm_lcase.contains("smf") {
            Some(String::from("smf"))
        } else if nm_lcase.contains("upf") {
            Some(String::from("upf"))
        }else{
            return Err(EntityError{ 
                kind: BACKSTAGE_ENTITY_SYSTEM.to_owned(),
                name: obj.name_any().clone(),
                message: "postgres cluster label missing system".to_owned(),
            });
        };
        // smf-postgres-cicd
        let system_name = format!("{}-postgres-{}", postgres_system.clone().unwrap(), 
                                            config.cluster.clone());

        Ok(Self {  
            api_version: BACKSTAGE_ENTITY_API_VERSION.to_string(),
            kind: BACKSTAGE_ENTITY_SYSTEM.to_string(),
            metadata: Metadata::from_annotations(&config.backstage, system_name),
            spec: SystemSpec {
                r#type: Some("service".to_owned()),
                owner: String::from(BACKSTAGE_DEFAULT_OWNER.to_owned()),
                domain: postgres_system,
                ..Default::default()
            }
        })
    }

    pub fn from_params(mt: Metadata, spec: SystemSpec) -> Result<Self, EntityError> {
        Ok(
            Self { 
                api_version: BACKSTAGE_ENTITY_API_VERSION.to_owned(), 
                kind: BACKSTAGE_ENTITY_SYSTEM.to_owned(), 
                metadata: mt, 
                spec: spec, 
            }
        )
    } 
}
// common trait for all Entities
pub trait BackstageEntity {
    // needed for dynamic casting to underlying types
    fn as_any(&self) -> &dyn Any;
    fn entity_type(&self) -> String;
    fn bse_to_string(&self) -> String;
}

impl Serialize for Box<dyn BackstageEntity> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where 
    S: Serializer,
    {
        if let Some(bs_res) = self.as_any().downcast_ref::<Resource>() {
            let mut state = serializer.serialize_struct("Resource", 4)?;
            state.serialize_field("apiVersion", &bs_res.api_version)?;
            state.serialize_field("kind", &bs_res.kind)?;
            state.serialize_field("metadata", &bs_res.metadata)?;
            state.serialize_field("spec", &bs_res.spec)?;
            state.end()
        } else if let Some(bs_gr) = self.as_any().downcast_ref::<Group>() {
            let mut state = serializer.serialize_struct("Group", 4)?;
            state.serialize_field("apiVersion", &bs_gr.api_version)?;
            state.serialize_field("kind", &bs_gr.kind)?;
            state.serialize_field("metadata", &bs_gr.metadata)?;
            state.serialize_field("spec", &bs_gr.spec)?;
            state.end()
        } else if let Some(bs_user) = self.as_any().downcast_ref::<User>() {
            let mut state = serializer.serialize_struct("User", 4)?;
            state.serialize_field("apiVersion", &bs_user.api_version)?;
            state.serialize_field("kind", &bs_user.kind)?;
            state.serialize_field("metadata", &bs_user.metadata)?;
            state.serialize_field("spec", &bs_user.spec)?;
            state.end()
        } else if let Some(bs_user) = self.as_any().downcast_ref::<Domain>() {
            let mut state = serializer.serialize_struct("User", 4)?;
            state.serialize_field("apiVersion", &bs_user.api_version)?;
            state.serialize_field("kind", &bs_user.kind)?;
            state.serialize_field("metadata", &bs_user.metadata)?;
            state.serialize_field("spec", &bs_user.spec)?;
            state.end()
        } else if let Some(bs_user) = self.as_any().downcast_ref::<System>() {
            let mut state = serializer.serialize_struct("User", 4)?;
            state.serialize_field("apiVersion", &bs_user.api_version)?;
            state.serialize_field("kind", &bs_user.kind)?;
            state.serialize_field("metadata", &bs_user.metadata)?;
            state.serialize_field("spec", &bs_user.spec)?;
            state.end()
        } else {
            Err(serde::ser::Error::custom("unknown BackstageEntity type"))
        }
    }
}


impl BackstageEntity for Resource {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn entity_type(&self) -> String {
        String::from("Resource")
    }

    fn bse_to_string(&self) -> String {
        match serde_json::to_string(&self) {
            Ok(res) => res,
            Err(_why) => "".to_owned()
        }
    }
}

impl BackstageEntity for Group {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn entity_type(&self) -> String {
        String::from("Group")
    }

    fn bse_to_string(&self) -> String {
        match serde_json::to_string(&self) {
            Ok(res) => res,
            Err(_why) => "".to_owned()
        }
    }
}

impl BackstageEntity for User {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn entity_type(&self) -> String {
        String::from("User")
    }

    fn bse_to_string(&self) -> String {
        match serde_json::to_string(&self) {
            Ok(res) => res,
            Err(_why) => "".to_owned()
        }
    }
}

impl BackstageEntity for Domain {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn entity_type(&self) -> String {
        String::from("Domain")
    }

    fn bse_to_string(&self) -> String {
        match serde_json::to_string(&self) {
            Ok(res) => res,
            Err(_why) => "".to_owned()
        }
    }
}

impl BackstageEntity for System {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn entity_type(&self) -> String {
        String::from("System")
    }

    fn bse_to_string(&self) -> String {
        match serde_json::to_string(&self) {
            Ok(res) => res,
            Err(_why) => "".to_owned()
        }
    }
}

#[derive(Debug)]
pub struct EntityError {
    pub kind: String,
    pub name: String,
    pub message: String,
}

impl fmt::Display for EntityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "kind: {} name: {} err={}", 
            self.kind, 
            self.name, 
            self.message)
    }
}
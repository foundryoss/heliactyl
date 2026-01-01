use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerSoftware {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub docker_images: HashMap<String, String>,
    pub startup_cmd: String,
    pub install_content: String,
    pub update_content: String,
    pub runtime: SoftwareRuntime,
    #[serde(default)]
    pub networking: Option<SoftwareNetworking>,
    pub variables: Vec<SoftwareVariable>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SoftwareNetworking {
    #[serde(default)]
    pub port_binds: HashMap<String, String>, // "3000": "auto", "8080": "auto"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SoftwareRuntime {
    #[serde(rename = "file-init")]
    pub file_init: String,
    #[serde(rename = "start-up")]
    pub start_up: String,
    pub stop: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SoftwareVariable {
    pub name: String,
    pub description: String,
    pub env_variable: String,
    pub default_value: String,
    pub user_viewable: bool,
    pub user_editable: bool,
    pub rules: String,
    pub field_type: String,
}

impl ServerSoftware {
    pub fn new(
        name: String,
        icon_url: Option<String>,
        docker_images: HashMap<String, String>,
        startup_cmd: String,
        install_content: String,
        update_content: String,
        runtime: SoftwareRuntime,
        networking: Option<SoftwareNetworking>,
        variables: Vec<SoftwareVariable>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: None,
            name,
            icon_url,
            docker_images,
            startup_cmd,
            install_content,
            update_content,
            runtime,
            networking,
            variables,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSoftwareRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub docker_images: HashMap<String, String>,
    pub startup_cmd: String,
    pub install_content: String,
    pub update_content: String,
    pub runtime: SoftwareRuntime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub networking: Option<SoftwareNetworking>,
    pub variables: Vec<SoftwareVariable>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateSoftwareRequest {
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub docker_images: Option<HashMap<String, String>>,
    pub startup_cmd: Option<String>,
    pub install_content: Option<String>,
    pub update_content: Option<String>,
    pub runtime: Option<SoftwareRuntime>,
    pub networking: Option<SoftwareNetworking>,
    pub variables: Option<Vec<SoftwareVariable>>,
}

use mongodb::{
    Client, Database, 
    options::{ClientOptions, ServerApi, ServerApiVersion},
    bson::{doc, oid::ObjectId},
};
use std::error::Error;
use crate::models::user::User;

#[derive(Clone)]
pub struct MongoClient {
    client: Client,
    db_name: String,
}

impl MongoClient {
    pub async fn new(uri: &str, db_name: &str) -> Result<Self, Box<dyn Error>> {
        let mut client_options = ClientOptions::parse(uri).await?;
        
        // Set the Stable API version for MongoDB Atlas
        let server_api = ServerApi::builder()
            .version(ServerApiVersion::V1)
            .build();
        client_options.server_api = Some(server_api);
        
        // Set server selection timeout
        client_options.server_selection_timeout = Some(std::time::Duration::from_secs(10));
        
        let client = Client::with_options(client_options)?;
        
        Ok(Self {
            client,
            db_name: db_name.to_string(),
        })
    }

    pub fn database(&self) -> Database {
        self.client.database(&self.db_name)
    }

    pub async fn ping(&self) -> Result<(), Box<dyn Error>> {
        self.client
            .database("admin")
            .run_command(mongodb::bson::doc! { "ping": 1 })
            .await?;
        Ok(())
    }

    pub fn get_client(&self) -> &Client {
        &self.client
    }

    // User management methods for admin panel
    pub async fn get_all_users(&self) -> Result<Vec<User>, Box<dyn Error + Send + Sync>> {
        use futures_util::TryStreamExt;
        let collection = self.database().collection::<User>("users");
        let cursor = collection.find(doc! {}).sort(doc! { "createdAt": -1 }).await?;
        let users: Vec<User> = cursor.try_collect().await?;
        Ok(users)
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<Option<User>, Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<User>("users");
        let object_id = ObjectId::parse_str(user_id)?;
        let user = collection.find_one(doc! { "_id": object_id }).await?;
        Ok(user)
    }

    pub async fn set_user_admin(&self, user_id: &str, is_admin: bool) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<User>("users");
        let object_id = ObjectId::parse_str(user_id)?;
        collection.update_one(
            doc! { "_id": object_id },
            doc! { "$set": { "isAdmin": is_admin, "updatedAt": mongodb::bson::DateTime::now() } }
        ).await?;
        Ok(())
    }

    pub async fn update_user(&self, user_id: &str, email: Option<&str>, username: Option<&str>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<User>("users");
        let object_id = ObjectId::parse_str(user_id)?;
        
        let mut update_doc = doc! { "updatedAt": mongodb::bson::DateTime::now() };
        if let Some(e) = email {
            update_doc.insert("email", e.to_lowercase());
        }
        if let Some(u) = username {
            update_doc.insert("username", u);
        }
        
        collection.update_one(
            doc! { "_id": object_id },
            doc! { "$set": update_doc }
        ).await?;
        Ok(())
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<User>("users");
        let object_id = ObjectId::parse_str(user_id)?;
        collection.delete_one(doc! { "_id": object_id }).await?;
        Ok(())
    }

    pub async fn reset_user_password(&self, user_id: &str, password_hash: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<User>("users");
        let object_id = ObjectId::parse_str(user_id)?;
        collection.update_one(
            doc! { "_id": object_id },
            doc! { "$set": { "passwordHash": password_hash, "updatedAt": mongodb::bson::DateTime::now() } }
        ).await?;
        Ok(())
    }

    // Node management methods
    pub async fn create_node(&self, node: &crate::models::node::Node) -> Result<String, Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::node::Node>("nodes");
        let result = collection.insert_one(node).await?;
        Ok(result.inserted_id.as_object_id().unwrap().to_hex())
    }

    pub async fn get_all_nodes(&self) -> Result<Vec<crate::models::node::Node>, Box<dyn Error + Send + Sync>> {
        use futures_util::TryStreamExt;
        let collection = self.database().collection::<crate::models::node::Node>("nodes");
        let cursor = collection.find(doc! {}).sort(doc! { "createdAt": -1 }).await?;
        let nodes: Vec<crate::models::node::Node> = cursor.try_collect().await?;
        Ok(nodes)
    }

    pub async fn get_node_by_id(&self, node_id: &str) -> Result<Option<crate::models::node::Node>, Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::node::Node>("nodes");
        let object_id = ObjectId::parse_str(node_id)?;
        let node = collection.find_one(doc! { "_id": object_id }).await?;
        Ok(node)
    }

    pub async fn update_node(&self, node_id: &str, update: &crate::models::node::UpdateNodeRequest) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::node::Node>("nodes");
        let object_id = ObjectId::parse_str(node_id)?;
        
        let mut update_doc = doc! { "updatedAt": mongodb::bson::DateTime::now() };
        
        if let Some(name) = &update.name {
            update_doc.insert("name", name);
        }
        if let Some(icon_url) = &update.icon_url {
            update_doc.insert("icon_url", icon_url);
        }
        if let Some(limits) = &update.limits {
            update_doc.insert("limits", mongodb::bson::to_bson(limits)?);
        }
        if let Some(network) = &update.network {
            update_doc.insert("network", mongodb::bson::to_bson(network)?);
        }
        if let Some(node_config) = &update.node_config {
            update_doc.insert("node_config", mongodb::bson::to_bson(node_config)?);
        }
        
        collection.update_one(
            doc! { "_id": object_id },
            doc! { "$set": update_doc }
        ).await?;
        Ok(())
    }

    pub async fn delete_node(&self, node_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::node::Node>("nodes");
        let object_id = ObjectId::parse_str(node_id)?;
        collection.delete_one(doc! { "_id": object_id }).await?;
        Ok(())
    }

    // Server Software management methods
    pub async fn create_software(&self, software: &crate::models::software::ServerSoftware) -> Result<String, Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::software::ServerSoftware>("server_software");
        let result = collection.insert_one(software).await?;
        Ok(result.inserted_id.as_object_id().unwrap().to_hex())
    }

    pub async fn get_all_software(&self) -> Result<Vec<crate::models::software::ServerSoftware>, Box<dyn Error + Send + Sync>> {
        use futures_util::TryStreamExt;
        let collection = self.database().collection::<crate::models::software::ServerSoftware>("server_software");
        let cursor = collection.find(doc! {}).sort(doc! { "createdAt": -1 }).await?;
        let software: Vec<crate::models::software::ServerSoftware> = cursor.try_collect().await?;
        Ok(software)
    }

    pub async fn get_software_by_id(&self, software_id: &str) -> Result<Option<crate::models::software::ServerSoftware>, Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::software::ServerSoftware>("server_software");
        let object_id = ObjectId::parse_str(software_id)?;
        let software = collection.find_one(doc! { "_id": object_id }).await?;
        Ok(software)
    }

    pub async fn update_software(&self, software_id: &str, update: &crate::models::software::UpdateSoftwareRequest) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::software::ServerSoftware>("server_software");
        let object_id = ObjectId::parse_str(software_id)?;
        
        let mut update_doc = doc! { "updatedAt": mongodb::bson::DateTime::now() };
        
        if let Some(name) = &update.name {
            update_doc.insert("name", name);
        }
        if let Some(icon_url) = &update.icon_url {
            update_doc.insert("icon_url", icon_url);
        }
        if let Some(docker_images) = &update.docker_images {
            update_doc.insert("docker_images", mongodb::bson::to_bson(docker_images)?);
        }
        if let Some(startup_cmd) = &update.startup_cmd {
            update_doc.insert("startup_cmd", startup_cmd);
        }
        if let Some(install_content) = &update.install_content {
            update_doc.insert("install_content", install_content);
        }
        if let Some(update_content) = &update.update_content {
            update_doc.insert("update_content", update_content);
        }
        if let Some(runtime) = &update.runtime {
            update_doc.insert("runtime", mongodb::bson::to_bson(runtime)?);
        }
        if let Some(variables) = &update.variables {
            update_doc.insert("variables", mongodb::bson::to_bson(variables)?);
        }
        
        collection.update_one(
            doc! { "_id": object_id },
            doc! { "$set": update_doc }
        ).await?;
        Ok(())
    }

    pub async fn delete_software(&self, software_id: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let collection = self.database().collection::<crate::models::software::ServerSoftware>("server_software");
        let object_id = ObjectId::parse_str(software_id)?;
        collection.delete_one(doc! { "_id": object_id }).await?;
        Ok(())
    }
}

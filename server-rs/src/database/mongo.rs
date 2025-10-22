use mongodb::{
    Client, Database, 
    options::{ClientOptions, ServerApi, ServerApiVersion}
};
use std::error::Error;

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
}

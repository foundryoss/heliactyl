use clap::Subcommand;
use mongodb::bson::doc;
use crate::database::mongo::MongoClient;
use crate::models::user::User;
use crate::heli::parse::HeliConfig;

#[derive(Subcommand)]
pub enum UsersCommands {
    /// List all users
    List {
        /// Config file path
        #[arg(short, long, default_value = "./config.heli")]
        config: String,
    },
    
    /// Set user admin status
    SetAdmin {
        /// User email or username
        identifier: String,
        
        /// Admin status (true/false)
        status: String,
        
        /// Config file path
        #[arg(short, long, default_value = "./config.heli")]
        config: String,
    },
    
    /// Get user details
    Get {
        /// User email or username
        identifier: String,
        
        /// Config file path
        #[arg(short, long, default_value = "./config.heli")]
        config: String,
    },
    
    /// Delete a user
    Delete {
        /// User email or username
        identifier: String,
        
        /// Config file path
        #[arg(short, long, default_value = "./config.heli")]
        config: String,
        
        /// Skip confirmation
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

pub async fn handle_users_command(cmd: UsersCommands) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        UsersCommands::List { config } => list_users(&config).await,
        UsersCommands::SetAdmin { identifier, status, config } => {
            set_admin_status(&identifier, &status, &config).await
        }
        UsersCommands::Get { identifier, config } => get_user(&identifier, &config).await,
        UsersCommands::Delete { identifier, config, yes } => {
            delete_user(&identifier, &config, yes).await
        }
    }
}

async fn get_mongo_client(config_path: &str) -> Result<MongoClient, Box<dyn std::error::Error>> {
    let heli_config = HeliConfig::parse(config_path)?;
    
    let mongo_uri = if let Ok(env_uri) = std::env::var("MONGODB") {
        env_uri
    } else {
        let db_host = heli_config.get_string("database.host").unwrap_or("localhost".to_string());
        let db_port = heli_config.get_int("database.port").unwrap_or(27017);
        let db_username = heli_config.get_string("database.username").unwrap_or("admin".to_string());
        let db_password = heli_config.get_string("database.password").unwrap_or("".to_string());
        
        if db_host.contains(".mongodb.net") {
            if db_password.is_empty() {
                format!("mongodb+srv://{}/?retryWrites=true&w=majority&appName=HeliactylRust", db_host)
            } else {
                format!("mongodb+srv://{}:{}@{}/?retryWrites=true&w=majority&appName=HeliactylRust", 
                    db_username, db_password, db_host)
            }
        } else {
            if db_password.is_empty() {
                format!("mongodb://{}:{}", db_host, db_port)
            } else {
                format!("mongodb://{}:{}@{}:{}", 
                    db_username, db_password, db_host, db_port)
            }
        }
    };
    
    let db_name = heli_config.get_string("database.name").unwrap_or("Heli-db".to_string());
    
    MongoClient::new(&mongo_uri, &db_name).await
}

async fn list_users(config_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mongo = get_mongo_client(config_path).await?;
    let db = mongo.database();
    let users_collection = db.collection::<User>("users");
    
    let mut cursor = users_collection.find(doc! {}).sort(doc! { "createdAt": -1 }).await?;
    
    println!("\n{:<25} {:<30} {:<30} {:<10}", "ID", "Email", "Username", "Admin");
    println!("{}", "-".repeat(95));
    
    use futures_util::StreamExt;
    let mut count = 0;
    while let Some(result) = cursor.next().await {
        if let Ok(user) = result {
            let id = user.id.map(|id| id.to_hex()).unwrap_or_default();
            let admin_status = if user.is_admin { "✓ Yes" } else { "No" };
            let id_display = if id.len() > 24 {
                format!("{}...", &id[..21])
            } else {
                id.clone()
            };
            let email_display = if user.email.len() > 30 {
                format!("{}...", &user.email[..27])
            } else {
                user.email.clone()
            };
            let username_display = if user.username.len() > 30 {
                format!("{}...", &user.username[..27])
            } else {
                user.username.clone()
            };
            
            println!(
                "{:<25} {:<30} {:<30} {:<10}",
                id_display,
                email_display,
                username_display,
                admin_status
            );
            count += 1;
        }
    }
    
    println!("\nTotal users: {}\n", count);
    Ok(())
}

async fn set_admin_status(
    identifier: &str,
    status_str: &str,
    config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse status string to boolean
    let status = match status_str.to_lowercase().as_str() {
        "true" | "1" | "yes" | "y" => true,
        "false" | "0" | "no" | "n" => false,
        _ => {
            eprintln!("\n✗ Invalid status value. Use: true/false, yes/no, 1/0\n");
            std::process::exit(1);
        }
    };
    let mongo = get_mongo_client(config_path).await?;
    let db = mongo.database();
    let users_collection = db.collection::<User>("users");
    
    // Find user by email or username
    let user = users_collection
        .find_one(doc! {
            "$or": [
                { "email": identifier },
                { "username": identifier }
            ]
        })
        .await?;
    
    match user {
        Some(user) => {
            let user_id = user.id.unwrap();
            let now = mongodb::bson::DateTime::now();
            
            // Update both isAdmin and updatedAt
            let result = users_collection
                .update_one(
                    doc! { "_id": user_id },
                    doc! { 
                        "$set": { 
                            "isAdmin": status,
                            "updatedAt": now
                        } 
                    }
                )
                .await?;
            
            if result.modified_count > 0 {
                println!(
                    "\n✓ User '{}' admin status set to: {}\n",
                    user.username,
                    if status { "true" } else { "false" }
                );
            } else {
                println!(
                    "\n⚠ User '{}' admin status was already: {}\n",
                    user.username,
                    if status { "true" } else { "false" }
                );
            }
            Ok(())
        }
        None => {
            eprintln!("\n✗ User '{}' not found\n", identifier);
            std::process::exit(1);
        }
    }
}

async fn get_user(identifier: &str, config_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mongo = get_mongo_client(config_path).await?;
    let db = mongo.database();
    let users_collection = db.collection::<User>("users");
    
    let user = users_collection
        .find_one(doc! {
            "$or": [
                { "email": identifier },
                { "username": identifier }
            ]
        })
        .await?;
    
    match user {
        Some(user) => {
            println!("\nUser Details:");
            println!("  ID:       {}", user.id.map(|id| id.to_hex()).unwrap_or_default());
            println!("  Email:    {}", user.email);
            println!("  Username: {}", user.username);
            println!("  Admin:    {}", if user.is_admin { "Yes" } else { "No" });
            println!("  Created:  {}", user.created_at);
            println!("  Updated:  {}", user.updated_at);
            if let Some(discord_id) = user.discord_id {
                println!("  Discord:  {}", discord_id);
            }
            println!();
            Ok(())
        }
        None => {
            eprintln!("\n✗ User '{}' not found\n", identifier);
            std::process::exit(1);
        }
    }
}

async fn delete_user(
    identifier: &str,
    config_path: &str,
    skip_confirm: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mongo = get_mongo_client(config_path).await?;
    let db = mongo.database();
    let users_collection = db.collection::<User>("users");
    
    let user = users_collection
        .find_one(doc! {
            "$or": [
                { "email": identifier },
                { "username": identifier }
            ]
        })
        .await?;
    
    match user {
        Some(user) => {
            if !skip_confirm {
                println!("\nAre you sure you want to delete user '{}'? (y/N): ", user.username);
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Cancelled.\n");
                    return Ok(());
                }
            }
            
            let user_id = user.id.unwrap();
            users_collection.delete_one(doc! { "_id": user_id }).await?;
            
            println!("\n✓ User '{}' deleted successfully\n", user.username);
            Ok(())
        }
        None => {
            eprintln!("\n✗ User '{}' not found\n", identifier);
            std::process::exit(1);
        }
    }
}

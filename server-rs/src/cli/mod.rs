pub mod users;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "heliactyl-rs")]
#[command(about = "Heliactyl Rust Server CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// User management commands
    #[command(subcommand)]
    Users(users::UsersCommands),
    
    /// Start the server
    Serve {
        /// Config file path
        #[arg(short, long, default_value = "./config.heli")]
        config: String,
    },
}

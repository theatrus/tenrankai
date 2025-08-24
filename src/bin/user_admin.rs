use clap::{Parser, Subcommand};
use std::path::Path;
use tenrankai::login::{User, UserDatabase};

#[derive(Parser)]
#[command(name = "user_admin")]
#[command(about = "Manage users for Tenrankai", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to users database file
    #[arg(short, long, default_value = "users.toml")]
    database: String,
}

#[derive(Subcommand)]
enum Commands {
    /// List all users
    List,
    /// Add a new user
    Add {
        /// Username (will be converted to lowercase)
        username: String,
        /// Email address
        email: String,
    },
    /// Remove a user
    Remove {
        /// Username to remove
        username: String,
    },
    /// Update a user's email
    Update {
        /// Username to update
        username: String,
        /// New email address
        email: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let db_path = Path::new(&cli.database);

    // Load or create database
    let mut db = if db_path.exists() {
        UserDatabase::load_from_file(db_path).await?
    } else {
        println!("Creating new user database at: {}", cli.database);
        UserDatabase::new()
    };

    match cli.command {
        Commands::List => {
            if db.users.is_empty() {
                println!("No users in database");
            } else {
                println!("Users in database:");
                for (_, user) in &db.users {
                    println!("  {} <{}>", user.username, user.email);
                }
            }
        }
        Commands::Add { username, email } => {
            let username = username.trim().to_lowercase();
            if db.get_user(&username).is_some() {
                eprintln!("Error: User '{}' already exists", username);
                std::process::exit(1);
            }
            
            let user = User {
                username: username.clone(),
                email: email.trim().to_string(),
            };
            
            db.add_user(user);
            db.save_to_file(db_path).await?;
            println!("Added user '{}' with email '{}'", username, email);
        }
        Commands::Remove { username } => {
            let username = username.trim().to_lowercase();
            if db.remove_user(&username).is_some() {
                db.save_to_file(db_path).await?;
                println!("Removed user '{}'", username);
            } else {
                eprintln!("Error: User '{}' not found", username);
                std::process::exit(1);
            }
        }
        Commands::Update { username, email } => {
            let username = username.trim().to_lowercase();
            if let Some(user) = db.users.get_mut(&username) {
                user.email = email.trim().to_string();
                db.save_to_file(db_path).await?;
                println!("Updated email for user '{}' to '{}'", username, email);
            } else {
                eprintln!("Error: User '{}' not found", username);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
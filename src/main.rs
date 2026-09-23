mod db;
mod models;
mod scheduler;
mod ui;
mod markdown;
mod tree;
mod outline;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::Command;
use std::path::PathBuf;

use db::Store;
use directories::ProjectDirs;

fn default_db_path() -> PathBuf {
    if let Some(proj) = ProjectDirs::from("dev", "vmargb", "myoso") {
        let dir = proj.data_dir();
        let _ = std::fs::create_dir_all(dir); // ensure it exists
        dir.join("flashcards.db")
    } else {
        // fallback if the OS gives us nothing (rare), stay relative
        PathBuf::from("flashcards.db")
    }
}

#[derive(Parser)]
#[command(
    name  = "myoso",
    about = "Multi-step spaced-repetition flashcards for the terminal.",
    version
)]
struct Cli {
    #[arg(long, global = true)]
    db: Option<PathBuf>, // path to sqlite

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Re-run the installer to fetch the newest release
    Update,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Some(Commands::Update) = cli.command {
        run_update()?;
        return Ok(());
    }

    let db_path = cli.db.unwrap_or_else(default_db_path);
    let store = Store::open(&db_path.to_string_lossy())?;
    ui::run_tui(&store)?;
    Ok(())
}

fn run_update() -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "irm https://raw.githubusercontent.com/vmargb/myoso/main/install.ps1 | iex",
            ])
            .status()?;

        if !status.success() {
            anyhow::bail!("update failed");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let status = Command::new("sh")
            .args([
                "-c",
                "curl -fsSL https://raw.githubusercontent.com/vmargb/myoso/main/install.sh | sh",
            ])
            .status()?;

        if !status.success() {
            anyhow::bail!("update failed");
        }
    }

    Ok(())
}

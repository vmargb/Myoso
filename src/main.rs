mod db;
mod models;
mod scheduler;
mod ui;
mod markdown;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::process::Command;
use std::path::PathBuf;

use db::Store;

#[derive(Parser)]
#[command(
    name  = "myoso",
    about = "Multi-step spaced-repetition flashcards for the terminal.",
    version
)]
struct Cli {
    #[arg(long, default_value = "flashcards.db", global = true)]
    db: PathBuf, // path to sqlite

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

    let db_path = cli.db.to_string_lossy().to_string();
    let store = Store::open(&db_path)?;
    store.seed_if_empty()?;
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

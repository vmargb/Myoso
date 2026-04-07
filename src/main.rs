mod db;
mod models;
mod scheduler;
mod ui;

use anyhow::Result;
use clap::Parser;
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
}

fn main() -> Result<()> {
    let cli     = Cli::parse();
    let db_path = cli.db.to_string_lossy().to_string();

    let store = Store::open(&db_path)?;
    store.seed_if_empty()?;

    ui::run_tui(&store)?;
    Ok(())
}

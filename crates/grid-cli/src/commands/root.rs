//! Root path management commands — `octo root show` / `octo root init`

use anyhow::Result;

use super::types::RootCommands;
use super::AppState;

/// Handle root subcommands
pub async fn handle_root(action: RootCommands, state: &AppState) -> Result<()> {
    match action {
        RootCommands::Show => show_root(state),
        RootCommands::Init => init_root(state),
    }
}

fn show_root(state: &AppState) -> Result<()> {
    let root = &state.grid_root;

    println!("GridRoot Path Summary");
    println!("=====================");
    println!();
    println!("Working directory:    {}", root.working_dir().display());
    println!("Project key:          {}", root.project_key());
    println!();
    println!("Global root:          {}", root.global_root().display());
    println!("  config.yaml:        {}", root.global_config().display());
    println!("  skills/:            {}", root.global_skills_dir().display());
    println!("  cache/:             {}", root.cache_dir().display());
    println!();
    println!("Project root:         {}", root.project_root().display());
    println!("  config.yaml:        {}", root.project_config().display());
    println!("  skills/:            {}", root.project_skills_dir().display());
    println!();
    println!("Project data:         {}", root.project_data_dir().display());
    println!("  grid.db:            {}", root.db_path().display());
    println!("  history/:           {}", root.history_dir().display());
    println!();
    println!("Resolved DB path:     {}", root.resolve_db_path().display());
    println!("  (actual in use):    {}", state.db_path.display());

    Ok(())
}

fn init_root(state: &AppState) -> Result<()> {
    state.grid_root.ensure_dirs()?;
    println!("GridRoot directories initialized.");
    println!();
    show_root(state)?;
    Ok(())
}

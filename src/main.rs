mod app;
mod config;
mod identity;
mod tui;

use app::NomadApp;
use tui::TuiApp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nomad = NomadApp::new().await?;
    let dest_hash = nomad.dest_hash();

    let mut tui = TuiApp::new(dest_hash)?;
    tui.run()?;

    Ok(())
}

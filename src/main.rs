mod app;
mod config;
mod identity;

use app::NomadApp;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut app = NomadApp::new().await?;
    app.run().await?;

    Ok(())
}

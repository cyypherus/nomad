mod app;
mod browser;
mod config;
pub mod conversation;
mod identity;
mod tui;

use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use app::NomadApp;
use tui::{NetworkEvent, TuiApp, TuiCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let nomad = Arc::new(Mutex::new(NomadApp::new().await?));
    let dest_hash = nomad.lock().await.dest_hash();

    let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>(100);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<TuiCommand>(100);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    let nomad_clone = nomad.clone();
    let event_tx_clone = event_tx.clone();
    let network_task = tokio::spawn(async move {
        let mut app = nomad_clone.lock().await;
        let mut announce_rx = app.announce_events().await;
        drop(app);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    break;
                }
                result = announce_rx.recv() => {
                    if let Ok(event) = result {
                        let dest = event.destination.lock().await;
                        let hash = dest.desc.address_hash;
                        let mut hash_bytes = [0u8; 16];
                        hash_bytes.copy_from_slice(hash.as_slice());
                        let _ = event_tx_clone.send(NetworkEvent::AnnounceReceived(hash_bytes)).await;
                    }
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        TuiCommand::Announce => {
                            let _ = event_tx_clone.send(NetworkEvent::Status("Announcing...".to_string())).await;
                            let mut app = nomad_clone.lock().await;
                            app.announce().await;
                            let _ = event_tx_clone.send(NetworkEvent::Status("Announced".to_string())).await;
                            let _ = event_tx_clone.send(NetworkEvent::AnnounceSent).await;
                        }
                    }
                }
            }
        }
    });

    let tui_result = tokio::task::spawn_blocking(move || {
        let mut tui = TuiApp::new(dest_hash, event_rx, cmd_tx)?;
        tui.run()
    })
    .await?;

    let _ = shutdown_tx.send(()).await;
    let _ = network_task.await;

    tui_result?;
    Ok(())
}

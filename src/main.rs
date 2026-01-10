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
        let app = nomad_clone.lock().await;
        let mut announce_rx = app.announce_events().await;
        let mut data_rx = app.received_data_events();
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

                        let mut app = nomad_clone.lock().await;
                        app.handle_announce(&event).await;
                    }
                }
                result = data_rx.recv() => {
                    if let Ok(data) = result {
                        let mut app = nomad_clone.lock().await;
                        if let Some(msg) = app.handle_received_message(&data) {
                            let peer = msg.source_hash;
                            let _ = event_tx_clone.send(NetworkEvent::MessageReceived(peer)).await;
                        }
                    }
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        TuiCommand::Announce => {
                            let _ = event_tx_clone.send(NetworkEvent::Status("Announcing...".to_string())).await;
                            let app = nomad_clone.lock().await;
                            app.announce().await;
                            let _ = event_tx_clone.send(NetworkEvent::Status("Announced".to_string())).await;
                            let _ = event_tx_clone.send(NetworkEvent::AnnounceSent).await;
                        }
                        TuiCommand::LoadConversations => {
                            let app = nomad_clone.lock().await;
                            if let Ok(convos) = app.list_conversations() {
                                let _ = event_tx_clone.send(NetworkEvent::ConversationsUpdated(convos)).await;
                            }
                        }
                        TuiCommand::LoadMessages(peer) => {
                            let app = nomad_clone.lock().await;
                            if let Ok(messages) = app.get_conversation(&peer) {
                                let _ = event_tx_clone.send(NetworkEvent::MessagesLoaded(messages)).await;
                            }
                        }
                        TuiCommand::SendMessage { content, destination } => {
                            let _ = event_tx_clone.send(NetworkEvent::Status("Sending...".to_string())).await;
                            let mut app = nomad_clone.lock().await;
                            match app.send_message(&destination, &content).await {
                                Ok(_) => {
                                    let _ = event_tx_clone.send(NetworkEvent::Status("Sent".to_string())).await;
                                    if let Ok(convos) = app.list_conversations() {
                                        let _ = event_tx_clone.send(NetworkEvent::ConversationsUpdated(convos)).await;
                                    }
                                }
                                Err(e) => {
                                    let _ = event_tx_clone.send(NetworkEvent::Status(format!("Failed: {}", e))).await;
                                }
                            }
                        }
                        TuiCommand::MarkConversationRead(peer) => {
                            let app = nomad_clone.lock().await;
                            let _ = app.mark_conversation_read(&peer);
                            if let Ok(convos) = app.list_conversations() {
                                let _ = event_tx_clone.send(NetworkEvent::ConversationsUpdated(convos)).await;
                            }
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

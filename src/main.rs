mod app;
mod browser;
mod config;
pub mod conversation;
mod identity;
mod network;
mod packet_audit;
mod tui;

use std::fs::File;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use app::NomadApp;
use network::{NetworkClient, NodeRegistry, PageStatus};
use simplelog::{Config as LogConfig, LevelFilter, WriteLogger};
use tui::{NetworkEvent, TuiApp, TuiCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(".nomad")?;
    let log_file = File::create(".nomad/nomad.log")?;
    WriteLogger::init(LevelFilter::Trace, LogConfig::default(), log_file)?;

    log::info!("Starting Nomad...");

    let nomad = Arc::new(Mutex::new(NomadApp::new().await?));
    let dest_hash = nomad.lock().await.dest_hash();

    let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>(100);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<TuiCommand>(100);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

    let transport = nomad.lock().await.transport().clone();
    let registry = NodeRegistry::new(".nomad/nodes.toml");
    let initial_nodes: Vec<_> = registry.all().into_iter().cloned().collect();
    let network_client = Arc::new(NetworkClient::new(transport.clone(), registry));

    let nomad_clone = nomad.clone();
    let network_client_clone = network_client.clone();
    let event_tx_clone = event_tx.clone();

    let network_task = tokio::spawn(async move {
        let app = nomad_clone.lock().await;
        let mut announce_rx = app.announce_events().await;
        let mut data_rx = app.received_data_events().await;
        let testnet = app.testnet_address().to_string();
        drop(app);

        let mut node_announces = network_client_clone.node_announces();

        log::info!("Network task started, connected to {}", testnet);
        let _ = event_tx_clone
            .send(NetworkEvent::Status(format!("Connected to {}", testnet)))
            .await;

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    log::info!("Shutdown signal received");
                    break;
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        TuiCommand::Announce => {
                            let _ = event_tx_clone.send(NetworkEvent::Status("Announcing...".to_string())).await;
                            let app = nomad_clone.lock().await;
                            app.announce().await;
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
                        TuiCommand::FetchPage { node, path } => {
                            let url = format!("{}:{}", node.hash_hex(), path);
                            let event_tx = event_tx_clone.clone();

                            let request = network_client_clone.fetch_page(&node, &path).await;
                            let mut status_rx = request.status_receiver();

                            tokio::spawn(async move {
                                loop {
                                    let status = status_rx.borrow().clone();
                                    let msg = match &status {
                                        PageStatus::RequestingPath => "Requesting path...".into(),
                                        PageStatus::WaitingForAnnounce => "Waiting for announce...".into(),
                                        PageStatus::PathFound { hops } => format!("Path found ({} hops)", hops),
                                        PageStatus::Connecting => "Connecting...".into(),
                                        PageStatus::LinkEstablished => "Link established".into(),
                                        PageStatus::SendingRequest => "Sending request...".into(),
                                        PageStatus::AwaitingResponse => "Awaiting response...".into(),
                                        PageStatus::Retrieving { parts_received, total_parts } => {
                                            format!("Retrieving... {}/{}", parts_received, total_parts)
                                        }
                                        PageStatus::Complete => {
                                            break;
                                        }
                                        PageStatus::Cancelled => {
                                            let _ = event_tx.send(NetworkEvent::Status("Cancelled".into())).await;
                                            return;
                                        }
                                        PageStatus::Failed(reason) => {
                                            let _ = event_tx.send(NetworkEvent::PageFailed { url: url.clone(), reason: reason.clone() }).await;
                                            return;
                                        }
                                    };
                                    let _ = event_tx.send(NetworkEvent::Status(msg)).await;

                                    if status_rx.changed().await.is_err() {
                                        break;
                                    }
                                }

                                match request.result().await {
                                    Ok(content) => {
                                        let _ = event_tx.send(NetworkEvent::PageReceived { url, content }).await;
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(NetworkEvent::PageFailed { url, reason: e }).await;
                                    }
                                }
                            });
                        }
                    }
                }
                result = announce_rx.recv() => {
                    if let Ok(event) = result {
                        let dest = event.destination.lock().await;
                        network_client_clone.handle_announce(&dest, event.app_data.as_slice()).await;
                        drop(dest);
                    }
                }
                result = node_announces.recv() => {
                    if let Ok(node) = result {
                        log::info!("Node announce: {} ({})", node.name, node.hash_hex());
                        let _ = event_tx_clone.send(NetworkEvent::NodeAnnounce(node)).await;
                    }
                }
                result = data_rx.recv() => {
                    if let Ok(data) = result {
                        let mut app = nomad_clone.lock().await;
                        if let Some(msg) = app.handle_received_message(&data) {
                            let peer = msg.source_hash;
                            log::info!("Message received from {}", hex::encode(peer));
                            let _ = event_tx_clone.send(NetworkEvent::MessageReceived(peer)).await;
                        }
                    }
                }
            }
        }
    });

    let tui_result = tokio::task::spawn_blocking(move || {
        let mut tui = TuiApp::new(dest_hash, initial_nodes, event_rx, cmd_tx)?;
        tui.run()
    })
    .await?;

    let _ = shutdown_tx.send(()).await;
    let _ = network_task.await;

    tui_result?;
    Ok(())
}

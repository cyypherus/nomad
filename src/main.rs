mod app;
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
    let relay_enabled = nomad.lock().await.relay_enabled();
    let registry = NodeRegistry::new(".nomad/nodes.toml");
    let initial_nodes: Vec<_> = registry.all().into_iter().cloned().collect();
    let network_client = Arc::new(NetworkClient::new(transport.clone(), registry));

    let nomad_clone = nomad.clone();
    let network_client_clone = network_client.clone();
    let event_tx_clone = event_tx.clone();

    let network_task = tokio::spawn(async move {
        let app = nomad_clone.lock().await;
        let mut announce_rx = app.announce_events();
        let mut data_rx = app.received_data_events();
        let iface_manager = app.transport().iface_manager();
        let stats = app.stats().clone();
        drop(app);

        let mut node_announces = network_client_clone.node_announces();
        let mut stats_interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut last_connected_count = 0usize;

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
                            log::info!("Announce command received");
                            let (transport, delivery_dest) = {
                                let app = nomad_clone.lock().await;
                                (app.transport().clone(), app.delivery_destination())
                            };
                            if let Some(dest) = delivery_dest {
                                transport.send_announce(&dest, None).await;
                            }
                            log::info!("Announce completed");
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
                        TuiCommand::FetchPage { node, path, form_data } => {
                            let url = format!("{}:{}", node.hash_hex(), path);
                            let event_tx = event_tx_clone.clone();

                            let request = network_client_clone.fetch(&node, &path, form_data).await;
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
                                    Ok(data) => {
                                        let content = String::from_utf8_lossy(&data).into_owned();
                                        let _ = event_tx.send(NetworkEvent::PageReceived { url, content }).await;
                                    }
                                    Err(e) => {
                                        let _ = event_tx.send(NetworkEvent::PageFailed { url, reason: e }).await;
                                    }
                                }
                            });
                        }
                        TuiCommand::DownloadFile { node, path, filename } => {
                            log::info!("Download requested: {} from {} path={}", filename, node.name, path);
                            let event_tx = event_tx_clone.clone();
                            let request = network_client_clone.fetch(&node, &path, std::collections::HashMap::new()).await;
                            let mut status_rx = request.status_receiver();
                            let filename_clone = filename.clone();

                            tokio::spawn(async move {
                                loop {
                                    let status = status_rx.borrow().clone();
                                    let msg = match &status {
                                        PageStatus::RequestingPath => format!("Downloading {}: Requesting path...", filename_clone),
                                        PageStatus::WaitingForAnnounce => format!("Downloading {}: Waiting for announce...", filename_clone),
                                        PageStatus::PathFound { hops } => format!("Downloading {}: Path found ({} hops)", filename_clone, hops),
                                        PageStatus::Connecting => format!("Downloading {}: Connecting...", filename_clone),
                                        PageStatus::LinkEstablished => format!("Downloading {}: Link established", filename_clone),
                                        PageStatus::SendingRequest => format!("Downloading {}: Sending request...", filename_clone),
                                        PageStatus::AwaitingResponse => format!("Downloading {}: Awaiting response...", filename_clone),
                                        PageStatus::Retrieving { parts_received, total_parts } => {
                                            format!("Downloading {}: {}/{}", filename_clone, parts_received, total_parts)
                                        }
                                        PageStatus::Complete => break,
                                        PageStatus::Cancelled => {
                                            let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                                filename: filename_clone,
                                                reason: "Cancelled".into(),
                                            }).await;
                                            return;
                                        }
                                        PageStatus::Failed(reason) => {
                                            let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                                filename: filename_clone,
                                                reason: reason.clone(),
                                            }).await;
                                            return;
                                        }
                                    };
                                    let _ = event_tx.send(NetworkEvent::Status(msg)).await;

                                    if status_rx.changed().await.is_err() {
                                        break;
                                    }
                                }

                                match request.result().await {
                                    Ok(data) => {
                                        let download_dir = std::path::Path::new(".nomad/downloads");
                                        if let Err(e) = std::fs::create_dir_all(download_dir) {
                                            let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                                filename,
                                                reason: format!("Failed to create downloads dir: {}", e),
                                            }).await;
                                            return;
                                        }

                                        let file_path = download_dir.join(&filename);
                                        log::info!("Writing {} bytes to {:?}", data.len(), file_path);
                                        match std::fs::write(&file_path, &data) {
                                            Ok(_) => {
                                                log::info!("Download complete: {:?}", file_path);
                                                let _ = event_tx.send(NetworkEvent::DownloadComplete {
                                                    filename,
                                                    path: file_path.display().to_string(),
                                                }).await;
                                            }
                                            Err(e) => {
                                                log::error!("Failed to write file: {}", e);
                                                let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                                    filename,
                                                    reason: format!("Failed to write file: {}", e),
                                                }).await;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Download failed: {}", e);
                                        let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                            filename,
                                            reason: e,
                                        }).await;
                                    }
                                }
                            });
                        }
                    }
                }
                result = announce_rx.recv() => {
                    if let Ok(event) = result {
                        let (desc, identity) = {
                            let guard = event.destination.lock().await;
                            (guard.desc, guard.identity)
                        };
                        network_client_clone.handle_announce(desc, identity, event.app_data.as_slice()).await;
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
                _ = stats_interval.tick() => {
                    let snapshot = stats.snapshot();
                    let _ = event_tx_clone.send(NetworkEvent::RelayStats(snapshot)).await;

                    let connected_count = iface_manager.lock().await.interfaces().filter(|i| i.connected).count();
                    if connected_count != last_connected_count {
                        last_connected_count = connected_count;
                        let status_msg = format!("Connected to {} interface(s)", connected_count);
                        log::info!("{}", status_msg);
                        let _ = event_tx_clone.send(NetworkEvent::Status(status_msg)).await;
                    }
                }
            }
        }
    });

    let tui_result = tokio::task::spawn_blocking(move || {
        let mut tui = TuiApp::new(dest_hash, initial_nodes, relay_enabled, event_rx, cmd_tx)?;
        tui.run()
    })
    .await?;

    let _ = shutdown_tx.send(()).await;
    let _ = network_task.await;

    tui_result?;
    Ok(())
}

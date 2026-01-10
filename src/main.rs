mod app;
mod browser;
mod config;
pub mod conversation;
mod identity;
mod node;
mod packet_audit;
mod tui;

use std::fs::File;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

use app::NomadApp;
use node::{is_node_announce, NodeClient, PageRequestResult};
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
    let (page_result_tx, mut page_result_rx) = mpsc::channel::<PageRequestResult>(100);

    let transport = nomad.lock().await.transport().clone();
    let node_client = Arc::new(NodeClient::new(transport, page_result_tx));

    let nomad_clone = nomad.clone();
    let node_client_clone = node_client.clone();
    let event_tx_clone = event_tx.clone();
    let transport_for_audit = nomad.lock().await.transport().clone();
    let network_task = tokio::spawn(async move {
        let app = nomad_clone.lock().await;
        let mut announce_rx = app.announce_events().await;
        let mut data_rx = app.received_data_events().await;
        let testnet = app.testnet_address().to_string();
        let our_dest = app.dest_hash();
        drop(app);
        
        let mut raw_packet_rx = transport_for_audit.lock().await.iface_rx();
        let our_dest_hash = reticulum::hash::AddressHash::new_from_slice(&our_dest);

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
                    log::info!("Received command: {:?}", cmd);
                    let _ = event_tx_clone.send(NetworkEvent::Status(format!("Got cmd: {:?}", cmd))).await;
                    match cmd {
                        TuiCommand::Announce => {
                            log::info!("Processing Announce command");
                            let _ = event_tx_clone.send(NetworkEvent::Status("Announcing...".to_string())).await;
                            let app = nomad_clone.lock().await;
                            app.announce().await;
                            log::info!("Announce sent");
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
                        TuiCommand::ConnectToNode { hash, path } => {
                            let _ = event_tx_clone.send(NetworkEvent::Status("Connecting...".to_string())).await;
                            if let Err(e) = node_client_clone.request_page(hash, path.clone()).await {
                                let url = format!("{}:{}", hex::encode(hash), path);
                                let _ = event_tx_clone.send(NetworkEvent::ConnectionFailed { url, reason: e }).await;
                            }
                        }
                    }
                }
                result = announce_rx.recv() => {
                    match &result {
                        Ok(event) => {
                            let dest = event.destination.lock().await;
                            let hash = dest.desc.address_hash;
                            let name_hash = dest.desc.name.as_name_hash_slice();
                            let is_node = is_node_announce(&dest);
                            log::debug!("Announce {} name_hash={} is_node={}", hash, hex::encode(name_hash), is_node);

                            if is_node {
                                let name = parse_display_name(event.app_data.as_slice());
                                log::info!("Node announce from {} name={:?}", hash, name);
                                node_client_clone.register_node(&dest).await;
                                let mut hash_bytes = [0u8; 16];
                                hash_bytes.copy_from_slice(hash.as_slice());
                                drop(dest);
                                let _ = event_tx_clone.send(NetworkEvent::AnnounceReceived { hash: hash_bytes, name }).await;
                            } else {
                                log::debug!("LXMF announce from {}", hash);
                                drop(dest);
                                let mut app = nomad_clone.lock().await;
                                app.handle_announce(event).await;
                            }
                        }
                        Err(e) => {
                            log::warn!("Announce channel error: {:?}", e);
                        }
                    }
                }
                result = data_rx.recv() => {
                    match &result {
                        Ok(data) => {
                            log::debug!("Data received, {} bytes", data.data.len());
                            let mut app = nomad_clone.lock().await;
                            if let Some(msg) = app.handle_received_message(data) {
                                let peer = msg.source_hash;
                                log::info!("Message received from {}", hex::encode(peer));
                                let _ = event_tx_clone.send(NetworkEvent::MessageReceived(peer)).await;
                            }
                        }
                        Err(e) => {
                            log::warn!("Data channel error: {:?}", e);
                        }
                    }
                }
                Some(result) = page_result_rx.recv() => {
                    match result {
                        PageRequestResult::Success { url, content } => {
                            let _ = event_tx_clone.send(NetworkEvent::PageReceived { url, content }).await;
                        }
                        PageRequestResult::Failed { url, reason } => {
                            let _ = event_tx_clone.send(NetworkEvent::ConnectionFailed { url, reason }).await;
                        }
                        PageRequestResult::TimedOut { url } => {
                            let _ = event_tx_clone.send(NetworkEvent::ConnectionFailed {
                                url,
                                reason: "Request timed out".to_string(),
                            }).await;
                        }
                    }
                }
                result = raw_packet_rx.recv() => {
                    if let Ok(msg) = result {
                        let packet = &msg.packet;
                        let ptype = packet.header.packet_type;
                        let ctx = packet.context;
                        let dest = &packet.destination;
                        
                        let is_for_us = dest.as_slice() == our_dest_hash.as_slice();
                        
                        let handled = match ptype {
                            reticulum::packet::PacketType::Announce => true,
                            reticulum::packet::PacketType::LinkRequest => is_for_us,
                            reticulum::packet::PacketType::Proof => {
                                if ctx == reticulum::packet::PacketContext::LinkRequestProof {
                                    log::info!("[AUDIT] LINK_PROOF received! dest={} - checking if this matches our pending link", dest);
                                }
                                true
                            },
                            reticulum::packet::PacketType::Data => is_for_us,
                        };
                        
                        if !handled {
                            log::debug!("[AUDIT] Packet not for us: type={:?} dest={}", ptype, dest);
                        } else {
                            log::info!("[AUDIT] Packet: type={:?} ctx={:?} dest={} for_us={}", ptype, ctx, dest, is_for_us);
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

fn parse_display_name(app_data: &[u8]) -> Option<String> {
    if app_data.is_empty() {
        return None;
    }

    // Version 0.5.0+ announce format: msgpack array where first element is display name
    if (app_data[0] >= 0x90 && app_data[0] <= 0x9f) || app_data[0] == 0xdc {
        // Try parsing as array of optional values
        if let Ok(data) = rmp_serde::from_slice::<Vec<Option<serde_bytes::ByteBuf>>>(app_data) {
            if let Some(Some(name_bytes)) = data.first() {
                return String::from_utf8(name_bytes.to_vec()).ok();
            }
        }
        // Try parsing as array of strings
        if let Ok(data) = rmp_serde::from_slice::<Vec<Option<String>>>(app_data) {
            if let Some(name) = data.first() {
                return name.clone();
            }
        }
        return None;
    }

    // Original announce format: raw UTF-8 string
    String::from_utf8(app_data.to_vec()).ok()
}

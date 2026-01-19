mod app;
mod config;
mod identity;
mod network;
mod tui;

use std::collections::HashMap;
use std::fs::File;
use std::sync::Arc;

use rinse::RequestError;

use tokio::sync::{mpsc, oneshot};

use app::NomadApp;
use network::{NetworkClient, NodeRegistry};
use simplelog::{Config as LogConfig, LevelFilter, WriteLogger};
use tui::{NetworkEvent, TuiApp, TuiCommand};

struct FetchReq {
    dest: [u8; 16],
    path: String,
    form_data: HashMap<String, String>,
    reply: oneshot::Sender<Result<Vec<u8>, String>>,
}

enum InternalCmd {
    Fetch(FetchReq),
    GetStats(oneshot::Sender<rinse::StatsSnapshot>),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(".nomad")?;
    let log_file = File::create(".nomad/nomad.log")?;
    WriteLogger::init(LevelFilter::Trace, LogConfig::default(), log_file)?;

    log::info!("Starting Nomad...");

    let (node, mut service, requester, dest_hash, relay_enabled) = {
        let mut nomad = NomadApp::new().await?;
        let dest_hash = nomad.dest_hash();
        let relay_enabled = nomad.relay_enabled();
        let node = nomad.take_node();
        let service = nomad.take_service();
        let requester = service.requester();
        (node, service, requester, dest_hash, relay_enabled)
    };

    let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>(100);
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<TuiCommand>(100);
    let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
    let (internal_tx, mut internal_rx) = mpsc::channel::<InternalCmd>(32);

    let registry = NodeRegistry::new(".nomad/nodes.toml");
    let initial_nodes: Vec<_> = registry.all().into_iter().cloned().collect();
    let network_client = Arc::new(NetworkClient::new(registry));

    let network_client_clone = network_client.clone();
    let event_tx_clone = event_tx.clone();
    let internal_tx_stats = internal_tx.clone();

    let node_task = tokio::spawn(async move {
        let node = Arc::try_unwrap(node)
            .ok()
            .expect("node still has multiple references")
            .into_inner();
        node.run().await;
    });

    let network_task = tokio::spawn(async move {
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
                            service.announce();
                            log::info!("Announce completed");
                            let _ = event_tx_clone.send(NetworkEvent::AnnounceSent).await;
                        }
                        TuiCommand::FetchPage { node, path, form_data } => {
                            log::info!("FetchPage command received: {} path={}", node.hash_hex(), path);
                            let url = format!("{}:{}", node.hash_hex(), path);
                            let event_tx = event_tx_clone.clone();
                            let internal_tx = internal_tx.clone();

                            tokio::spawn(async move {
                                log::info!("Spawned fetch task for {}", url);
                                let _ = event_tx.send(NetworkEvent::Status("Sending request...".into())).await;

                                let (reply_tx, reply_rx) = oneshot::channel();
                                log::info!("Sending InternalCmd::Fetch");
                                let _ = internal_tx.send(InternalCmd::Fetch(FetchReq {
                                    dest: node.hash,
                                    path: path.clone(),
                                    form_data,
                                    reply: reply_tx,
                                })).await;
                                log::info!("InternalCmd::Fetch sent, waiting for reply");

                                let _ = event_tx.send(NetworkEvent::Status("Awaiting response...".into())).await;

                                match reply_rx.await {
                                    Ok(Ok(data)) => {
                                        let content = String::from_utf8_lossy(&data).into_owned();
                                        let _ = event_tx.send(NetworkEvent::PageReceived { url, content }).await;
                                    }
                                    Ok(Err(e)) => {
                                        let _ = event_tx.send(NetworkEvent::PageFailed { url, reason: e }).await;
                                    }
                                    Err(_) => {
                                        let _ = event_tx.send(NetworkEvent::PageFailed { url, reason: "Request cancelled".into() }).await;
                                    }
                                }
                            });
                        }
                        TuiCommand::DownloadFile { node, path, filename } => {
                            log::info!("Download requested: {} from {} path={}", filename, node.name, path);
                            let event_tx = event_tx_clone.clone();
                            let internal_tx = internal_tx.clone();

                            tokio::spawn(async move {
                                let _ = event_tx.send(NetworkEvent::Status(format!("Downloading {}...", filename))).await;

                                let (reply_tx, reply_rx) = oneshot::channel();
                                let _ = internal_tx.send(InternalCmd::Fetch(FetchReq {
                                    dest: node.hash,
                                    path,
                                    form_data: HashMap::new(),
                                    reply: reply_tx,
                                })).await;

                                match reply_rx.await {
                                    Ok(Ok(data)) => {
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
                                    Ok(Err(e)) => {
                                        log::error!("Download failed: {}", e);
                                        let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                            filename,
                                            reason: e,
                                        }).await;
                                    }
                                    Err(_) => {
                                        let _ = event_tx.send(NetworkEvent::DownloadFailed {
                                            filename,
                                            reason: "Request cancelled".into(),
                                        }).await;
                                    }
                                }
                            });
                        }
                    }
                }
                Some(cmd) = internal_rx.recv() => {
                    match cmd {
                        InternalCmd::Fetch(req) => {
                            log::info!("Processing fetch request: dest={} path={}", hex::encode(req.dest), req.path);
                            let request_data = build_page_request(&req.form_data);
                            let requester = requester.clone();
                            let path = req.path.clone();

                            tokio::spawn(async move {
                                log::info!("Calling requester.request()");
                                let result = match requester.request(req.dest, &path, &request_data).await {
                                    Ok(response) => {
                                        log::info!("Got response: {} bytes", response.len());
                                        parse_response(&response)
                                    }
                                    Err(e) => {
                                        log::error!("Request failed: {:?}", e);
                                        let msg = match e {
                                            RequestError::Timeout => "Request timed out".to_string(),
                                            RequestError::LinkFailed => "Failed to establish link".to_string(),
                                            RequestError::LinkClosed => "Link closed".to_string(),
                                            RequestError::TransferFailed => "Transfer failed".to_string(),
                                        };
                                        Err(msg)
                                    }
                                };
                                log::info!("Sending reply");
                                let _ = req.reply.send(result);
                            });
                        }
                        InternalCmd::GetStats(reply) => {
                            let stats = service.stats().await;
                            let _ = reply.send(stats);
                        }
                    }
                }
                Some(destinations) = service.recv_destinations_changed() => {
                    network_client_clone.handle_destinations_changed(destinations).await;
                }
            }
        }
    });

    let mut node_announces = network_client.node_announces();
    let event_tx_announce = event_tx.clone();

    let announce_task = tokio::spawn(async move {
        while let Ok(node) = node_announces.recv().await {
            log::info!("Node announce: {} ({})", node.name, node.hash_hex());
            let _ = event_tx_announce
                .send(NetworkEvent::NodeAnnounce(node))
                .await;
        }
    });

    let event_tx_stats = event_tx.clone();
    let stats_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let (reply_tx, reply_rx) = oneshot::channel();
            if internal_tx_stats
                .send(InternalCmd::GetStats(reply_tx))
                .await
                .is_ok()
            {
                if let Ok(stats) = reply_rx.await {
                    let _ = event_tx_stats.send(NetworkEvent::RelayStats(stats)).await;
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
    announce_task.abort();
    stats_task.abort();
    node_task.abort();

    tui_result?;
    Ok(())
}

fn build_page_request(form_data: &HashMap<String, String>) -> Vec<u8> {
    if form_data.is_empty() {
        Vec::new()
    } else {
        rmp_serde::to_vec(form_data).unwrap_or_default()
    }
}

fn parse_response(data: &[u8]) -> Result<Vec<u8>, String> {
    if let Ok(response) = rmp_serde::from_slice::<(f64, Vec<u8>, Option<Vec<u8>>)>(data) {
        return response.2.ok_or_else(|| "No content in response".into());
    }

    if let Ok(response) =
        rmp_serde::from_slice::<(serde_bytes::ByteBuf, serde_bytes::ByteBuf)>(data)
    {
        return Ok(response.1.to_vec());
    }

    Ok(data.to_vec())
}

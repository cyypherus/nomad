use reticulum::destination::link::LinkEvent;
use reticulum::destination::{DestinationDesc, DestinationName, SingleOutputDestination};
use reticulum::hash::AddressHash;
use reticulum::identity::HashIdentity;
use reticulum::packet::PacketContext;
use reticulum::transport::Transport;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, mpsc, Mutex};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub enum PageRequestResult {
    Success { url: String, content: String },
    Failed { url: String, reason: String },
    TimedOut { url: String },
}

struct PendingRequest {
    url: String,
}

pub struct NodeClient {
    transport: Arc<Mutex<Transport>>,
    known_nodes: Arc<Mutex<HashMap<[u8; 16], DestinationDesc>>>,
    pending: Arc<Mutex<HashMap<AddressHash, PendingRequest>>>,
    result_tx: mpsc::Sender<PageRequestResult>,
}

impl NodeClient {
    pub fn new(
        transport: Arc<Mutex<Transport>>,
        result_tx: mpsc::Sender<PageRequestResult>,
    ) -> Self {
        Self {
            transport,
            known_nodes: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            result_tx,
        }
    }

    pub async fn register_node(&self, lxmf_dest: &SingleOutputDestination) {
        let identity = lxmf_dest.desc.identity;
        let node_name = DestinationName::new("nomadnetwork", "node");
        let node_hash = compute_address_hash(&identity, &node_name);

        let mut hash_bytes = [0u8; 16];
        hash_bytes.copy_from_slice(node_hash.as_slice());

        let node_desc = DestinationDesc {
            identity,
            address_hash: node_hash,
            name: node_name,
        };

        self.known_nodes.lock().await.insert(hash_bytes, node_desc);
    }

    pub async fn request_page(&self, lxmf_hash: [u8; 16], path: String) -> Result<(), String> {
        let node_desc = {
            let nodes = self.known_nodes.lock().await;
            nodes.get(&lxmf_hash).cloned()
        };

        let node_desc = match node_desc {
            Some(d) => d,
            None => return Err("Unknown node - no announce received".to_string()),
        };

        let url = format!("{}:{}", hex::encode(lxmf_hash), path);

        let transport = self.transport.lock().await;
        let link = transport.link(node_desc).await;
        let link_id = *link.lock().await.id();

        self.pending
            .lock()
            .await
            .insert(link_id, PendingRequest { url: url.clone() });

        let mut link_events = transport.out_link_events();
        drop(transport);

        let pending = self.pending.clone();
        let transport = self.transport.clone();
        let result_tx = self.result_tx.clone();

        tokio::spawn(async move {
            let timeout = tokio::time::sleep(REQUEST_TIMEOUT);
            tokio::pin!(timeout);

            loop {
                tokio::select! {
                    _ = &mut timeout => {
                        let mut pending = pending.lock().await;
                        if let Some(req) = pending.remove(&link_id) {
                            let _ = result_tx.send(PageRequestResult::TimedOut { url: req.url }).await;
                        }
                        break;
                    }
                    result = link_events.recv() => {
                        match result {
                            Ok(event_data) if event_data.id == link_id => {
                                match event_data.event {
                                    LinkEvent::Activated => {
                                        if let Err(e) = send_page_request(&transport, &link, &path).await {
                                            let mut pending = pending.lock().await;
                                            if let Some(req) = pending.remove(&link_id) {
                                                let _ = result_tx.send(PageRequestResult::Failed {
                                                    url: req.url,
                                                    reason: e,
                                                }).await;
                                            }
                                            break;
                                        }
                                    }
                                    LinkEvent::Data(payload) => {
                                        let mut pending = pending.lock().await;
                                        if let Some(req) = pending.remove(&link_id) {
                                            match parse_page_response(payload.as_slice()) {
                                                Ok(content) => {
                                                    let _ = result_tx.send(PageRequestResult::Success {
                                                        url: req.url,
                                                        content,
                                                    }).await;
                                                }
                                                Err(e) => {
                                                    let _ = result_tx.send(PageRequestResult::Failed {
                                                        url: req.url,
                                                        reason: e,
                                                    }).await;
                                                }
                                            }
                                        }
                                        break;
                                    }
                                    LinkEvent::Closed => {
                                        let mut pending = pending.lock().await;
                                        if let Some(req) = pending.remove(&link_id) {
                                            let _ = result_tx.send(PageRequestResult::Failed {
                                                url: req.url,
                                                reason: "Link closed".to_string(),
                                            }).await;
                                        }
                                        break;
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(broadcast::error::RecvError::Closed) => break,
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

fn compute_address_hash(
    identity: &reticulum::identity::Identity,
    name: &DestinationName,
) -> AddressHash {
    let hash = Sha256::new()
        .chain_update(name.as_name_hash_slice())
        .chain_update(identity.as_address_hash_slice())
        .finalize();

    AddressHash::new_from_slice(&hash[..16])
}

async fn send_page_request(
    transport: &Arc<Mutex<Transport>>,
    link: &Arc<Mutex<reticulum::destination::link::Link>>,
    path: &str,
) -> Result<(), String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let path_hash = compute_path_hash(path);

    let request_data: (f64, Vec<u8>, Option<()>) = (timestamp, path_hash.to_vec(), None);
    let packed = rmp_serde::to_vec(&request_data).map_err(|e| e.to_string())?;

    let link_guard = link.lock().await;
    let mut packet = link_guard
        .data_packet(&packed)
        .map_err(|e| format!("{:?}", e))?;
    packet.context = PacketContext::Request;
    drop(link_guard);

    transport.lock().await.send_packet(packet).await;
    Ok(())
}

fn compute_path_hash(path: &str) -> [u8; 16] {
    let hash = Sha256::digest(path.as_bytes());
    let mut result = [0u8; 16];
    result.copy_from_slice(&hash[..16]);
    result
}

fn parse_page_response(data: &[u8]) -> Result<String, String> {
    let response: (f64, Vec<u8>, Option<Vec<u8>>) =
        rmp_serde::from_slice(data).map_err(|e| format!("Failed to parse response: {}", e))?;

    let content_bytes = response.2.ok_or("No content in response")?;
    String::from_utf8(content_bytes).map_err(|e| format!("Invalid UTF-8: {}", e))
}

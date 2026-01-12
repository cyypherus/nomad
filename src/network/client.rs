use crate::network::node_registry::NodeRegistry;
use crate::network::page_request::{PageRequest, PageRequestHandle, PageStatus};
use crate::network::types::{IdentityInfo, NodeInfo, PeerInfo};

use reticulum::destination::link::{Link, LinkEvent};
use reticulum::destination::{DestinationDesc, DestinationName, SingleOutputDestination};
use reticulum::hash::AddressHash;
use reticulum::packet::PacketContext;
use reticulum::resource::{ResourceHandleResult, ResourceManager};
use reticulum::transport::Transport;

use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::time::timeout;

const PATH_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const LINK_TIMEOUT: Duration = Duration::from_secs(30);

pub struct NetworkClient {
    transport: Arc<Transport>,
    registry: Arc<RwLock<NodeRegistry>>,
    known_destinations: Arc<Mutex<HashMap<[u8; 16], DestinationDesc>>>,
    node_announce_tx: broadcast::Sender<NodeInfo>,
    peer_announce_tx: broadcast::Sender<PeerInfo>,
}

impl NetworkClient {
    pub fn new(transport: Arc<Transport>, registry: NodeRegistry) -> Self {
        let (node_announce_tx, _) = broadcast::channel(64);
        let (peer_announce_tx, _) = broadcast::channel(64);

        Self {
            transport,
            registry: Arc::new(RwLock::new(registry)),
            known_destinations: Arc::new(Mutex::new(HashMap::new())),
            node_announce_tx,
            peer_announce_tx,
        }
    }

    pub fn node_announces(&self) -> broadcast::Receiver<NodeInfo> {
        self.node_announce_tx.subscribe()
    }

    pub fn peer_announces(&self) -> broadcast::Receiver<PeerInfo> {
        self.peer_announce_tx.subscribe()
    }

    pub async fn handle_announce(
        &self,
        desc: DestinationDesc,
        identity: reticulum::identity::Identity,
        app_data: &[u8],
    ) {
        let mut hash = [0u8; 16];
        hash.copy_from_slice(desc.address_hash.as_slice());

        let mut public_key = [0u8; 32];
        public_key.copy_from_slice(identity.public_key_bytes());

        let mut verifying_key = [0u8; 32];
        verifying_key.copy_from_slice(identity.verifying_key.as_bytes());

        let identity_info = IdentityInfo {
            public_key,
            verifying_key,
        };

        self.known_destinations.lock().await.insert(hash, desc);

        if is_node_announce_desc(&desc) {
            let name = parse_display_name(app_data).unwrap_or_else(|| "Unknown".into());

            let node = NodeInfo {
                hash,
                name: name.clone(),
                identity: identity_info,
            };

            {
                let mut reg = self.registry.write().await;
                reg.save(node.clone());
            }

            let _ = self.node_announce_tx.send(node);
        } else {
            let name = parse_display_name(app_data);

            let peer = PeerInfo {
                hash,
                name,
                identity: identity_info,
            };

            let _ = self.peer_announce_tx.send(peer);
        }
    }

    pub async fn fetch_page(
        &self,
        node: &NodeInfo,
        path: &str,
        form_data: std::collections::HashMap<String, String>,
    ) -> PageRequest {
        let (handle, request) = PageRequestHandle::new();

        let transport = self.transport.clone();
        let known_destinations = self.known_destinations.clone();
        let node = node.clone();
        let path = path.to_string();

        tokio::spawn(async move {
            if let Err(e) = fetch_page_inner(
                transport,
                known_destinations,
                &node,
                &path,
                form_data,
                handle,
            )
            .await
            {
                log::error!("fetch_page failed: {}", e);
            }
        });

        request
    }

    pub async fn registry(&self) -> tokio::sync::RwLockReadGuard<'_, NodeRegistry> {
        self.registry.read().await
    }

    pub async fn registry_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, NodeRegistry> {
        self.registry.write().await
    }
}

macro_rules! check_cancelled {
    ($handle:expr) => {
        if $handle.is_cancelled() {
            $handle.cancelled();
            return Ok(());
        }
    };
}

async fn fetch_page_inner(
    transport: Arc<Transport>,
    known_destinations: Arc<Mutex<HashMap<[u8; 16], DestinationDesc>>>,
    node: &NodeInfo,
    path: &str,
    form_data: std::collections::HashMap<String, String>,
    handle: PageRequestHandle,
) -> Result<(), String> {
    let address_hash = AddressHash::from_bytes(&node.hash);

    log::debug!("fetch_page: checking has_path for {}", address_hash);
    let has_path = transport.has_path(&address_hash).await;
    log::debug!("fetch_page: has_path returned {}", has_path);

    if !has_path {
        handle.set_status(PageStatus::RequestingPath);
        log::debug!("fetch_page: requesting path");
        transport.request_path(&address_hash).await;
        log::debug!("fetch_page: request_path completed");

        handle.set_status(PageStatus::WaitingForAnnounce);
        let wait_result =
            wait_for_path(&transport, &address_hash, &handle, PATH_REQUEST_TIMEOUT).await;

        if !wait_result {
            check_cancelled!(handle);
            handle.fail("No path to node - try again later".into());
            return Ok(());
        }
    }

    check_cancelled!(handle);

    if let Some(hops) = transport.path_hops(&address_hash).await {
        handle.set_status(PageStatus::PathFound { hops });
    }

    let dest_desc = {
        let known = known_destinations.lock().await;
        known.get(&node.hash).cloned()
    }
    .unwrap_or_else(|| node.to_destination_desc());

    check_cancelled!(handle);
    handle.set_status(PageStatus::Connecting);

    let mut link_events = transport.out_link_events();
    let link = transport.link(dest_desc).await;
    let link_id = *link.lock().await.id();
    let already_active =
        link.lock().await.status() == reticulum::destination::link::LinkStatus::Active;

    if !already_active {
        let link_result =
            wait_for_link_activation(&mut link_events, &link_id, &handle, LINK_TIMEOUT).await;

        match link_result {
            LinkWaitResult::Activated => {}
            LinkWaitResult::Closed => {
                check_cancelled!(handle);
                handle.fail("Link closed".into());
                return Ok(());
            }
            LinkWaitResult::Timeout => {
                check_cancelled!(handle);
                handle.fail("Connection timed out".into());
                return Ok(());
            }
        }
    }

    handle.set_status(PageStatus::LinkEstablished);
    handle.set_status(PageStatus::SendingRequest);

    let request_result = send_page_request(&transport, &link, path, &form_data).await;
    if let Err(e) = request_result {
        handle.fail(e);
        return Ok(());
    }

    handle.set_status(PageStatus::AwaitingResponse);

    let mut resource_manager = ResourceManager::new();
    let mut current_resource_hash: Option<reticulum::hash::Hash> = None;
    let mut total_parts: u32 = 0;

    loop {
        check_cancelled!(handle);

        let event = timeout(Duration::from_secs(60), link_events.recv()).await;

        match event {
            Ok(Ok(event_data)) if event_data.id == link_id => match event_data.event {
                LinkEvent::Data(payload) => match parse_page_response(payload.as_slice()) {
                    Ok(content) => {
                        save_page_content(&content);
                        handle.complete(content);
                        return Ok(());
                    }
                    Err(e) => {
                        handle.fail(e);
                        return Ok(());
                    }
                },
                LinkEvent::ResourcePacket { context, data } => {
                    let link_guard = link.lock().await;
                    let decrypt_fn = |ciphertext: &[u8]| -> Option<Vec<u8>> {
                        let mut buf = vec![0u8; ciphertext.len() + 64];
                        link_guard
                            .decrypt(ciphertext, &mut buf)
                            .ok()
                            .map(|s| s.to_vec())
                    };
                    let encrypt_fn = |plaintext: &[u8]| -> Option<Vec<u8>> {
                        let mut buf = vec![0u8; plaintext.len() + 64];
                        link_guard
                            .encrypt(plaintext, &mut buf)
                            .ok()
                            .map(|s| s.to_vec())
                    };

                    let result = resource_manager.handle_packet(
                        &reticulum::packet::Packet {
                            header: Default::default(),
                            ifac: None,
                            destination: link_id,
                            transport: None,
                            context,
                            data: {
                                let mut buf = reticulum::packet::PacketDataBuffer::new();
                                buf.safe_write(&data);
                                buf
                            },
                        },
                        &link_id,
                        decrypt_fn,
                    );

                    match result {
                        ResourceHandleResult::RequestParts(hash) => {
                            current_resource_hash = Some(hash);
                            if let Some(info) = resource_manager.resource_info(&hash) {
                                total_parts = info.total_parts;
                                handle.set_status(PageStatus::Retrieving {
                                    parts_received: info.received_count,
                                    total_parts,
                                });
                            }
                            if let Some(request_packet) =
                                resource_manager.create_request_packet(&hash, &link_id, encrypt_fn)
                            {
                                drop(link_guard);
                                transport.send_packet(request_packet).await;
                            }
                        }
                        ResourceHandleResult::Assemble(hash) => {
                            if let Some(info) = resource_manager.resource_info(&hash) {
                                handle.set_status(PageStatus::Retrieving {
                                    parts_received: info.total_parts,
                                    total_parts: info.total_parts,
                                });
                            }
                            if let Some((data, proof_packet)) =
                                resource_manager.assemble_and_prove(&hash, &link_id, decrypt_fn)
                            {
                                drop(link_guard);
                                transport.send_packet(proof_packet).await;

                                match parse_resource_content(&data) {
                                    Ok(content) => {
                                        save_page_content(&content);
                                        handle.complete(content);
                                        return Ok(());
                                    }
                                    Err(e) => {
                                        handle.fail(e);
                                        return Ok(());
                                    }
                                }
                            } else {
                                handle.fail("Failed to assemble resource".into());
                                return Ok(());
                            }
                        }
                        ResourceHandleResult::None => {
                            if let Some(ref hash) = current_resource_hash {
                                if let Some(info) = resource_manager.resource_info(hash) {
                                    handle.set_status(PageStatus::Retrieving {
                                        parts_received: info.received_count,
                                        total_parts: info.total_parts,
                                    });
                                }
                            }
                        }
                    }
                }
                LinkEvent::Closed => {
                    handle.fail("Link closed".into());
                    return Ok(());
                }
                _ => {}
            },
            Ok(Ok(_)) => {}
            Ok(Err(_)) => {
                handle.fail("Link events channel closed".into());
                return Ok(());
            }
            Err(_) => {
                handle.fail("Request timed out".into());
                return Ok(());
            }
        }
    }
}

async fn wait_for_path(
    transport: &Arc<Transport>,
    address_hash: &AddressHash,
    handle: &PageRequestHandle,
    timeout_duration: Duration,
) -> bool {
    let start = std::time::Instant::now();
    let check_interval = Duration::from_millis(100);

    while start.elapsed() < timeout_duration {
        if handle.is_cancelled() {
            return false;
        }
        if transport.has_path(address_hash).await {
            return true;
        }
        tokio::time::sleep(check_interval).await;
    }

    false
}

enum LinkWaitResult {
    Activated,
    Closed,
    Timeout,
}

async fn wait_for_link_activation(
    link_events: &mut broadcast::Receiver<reticulum::destination::link::LinkEventData>,
    link_id: &AddressHash,
    handle: &PageRequestHandle,
    timeout_duration: Duration,
) -> LinkWaitResult {
    let deadline = tokio::time::Instant::now() + timeout_duration;

    loop {
        if handle.is_cancelled() {
            return LinkWaitResult::Closed;
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return LinkWaitResult::Timeout;
        }

        match timeout(remaining, link_events.recv()).await {
            Ok(Ok(event_data)) if event_data.id == *link_id => match event_data.event {
                LinkEvent::Activated => return LinkWaitResult::Activated,
                LinkEvent::Closed => return LinkWaitResult::Closed,
                _ => {}
            },
            Ok(Ok(_)) => {}
            Ok(Err(_)) => return LinkWaitResult::Closed,
            Err(_) => return LinkWaitResult::Timeout,
        }
    }
}

async fn send_page_request(
    transport: &Arc<Transport>,
    link: &Arc<Mutex<Link>>,
    path: &str,
    form_data: &std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);

    let path_hash = compute_path_hash(path);

    let packed = if form_data.is_empty() {
        let request_data: (f64, serde_bytes::ByteBuf, Option<()>) = (
            timestamp,
            serde_bytes::ByteBuf::from(path_hash.to_vec()),
            None,
        );
        rmp_serde::to_vec(&request_data).map_err(|e| e.to_string())?
    } else {
        let request_data: (
            f64,
            serde_bytes::ByteBuf,
            &std::collections::HashMap<String, String>,
        ) = (
            timestamp,
            serde_bytes::ByteBuf::from(path_hash.to_vec()),
            form_data,
        );
        rmp_serde::to_vec(&request_data).map_err(|e| e.to_string())?
    };

    let link_guard = link.lock().await;
    let mut packet = link_guard
        .data_packet(&packed)
        .map_err(|e| format!("{:?}", e))?;
    packet.context = PacketContext::Request;
    drop(link_guard);

    transport.send_packet(packet).await;
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

fn parse_resource_content(data: &[u8]) -> Result<String, String> {
    let response: (serde_bytes::ByteBuf, serde_bytes::ByteBuf) = rmp_serde::from_slice(data)
        .map_err(|e| format!("Failed to parse resource response: {}", e))?;

    String::from_utf8(response.1.to_vec()).map_err(|e| format!("Invalid UTF-8: {}", e))
}

fn save_page_content(content: &str) {
    if let Err(e) = std::fs::write(".nomad/last_page.mu", content) {
        log::warn!("Failed to save page: {}", e);
    }
}

fn is_node_announce_desc(desc: &DestinationDesc) -> bool {
    let expected = DestinationName::new("nomadnetwork", "node");
    desc.name.as_name_hash_slice() == expected.as_name_hash_slice()
}

fn parse_display_name(app_data: &[u8]) -> Option<String> {
    if app_data.is_empty() {
        return None;
    }

    if (app_data[0] >= 0x90 && app_data[0] <= 0x9f) || app_data[0] == 0xdc {
        if let Ok(data) = rmp_serde::from_slice::<Vec<Option<serde_bytes::ByteBuf>>>(app_data) {
            if let Some(Some(name_bytes)) = data.first() {
                return String::from_utf8(name_bytes.to_vec()).ok();
            }
        }
        if let Ok(data) = rmp_serde::from_slice::<Vec<Option<String>>>(app_data) {
            if let Some(name) = data.first() {
                return name.clone();
            }
        }
        return None;
    }

    String::from_utf8(app_data.to_vec()).ok()
}

use crate::network::node_registry::NodeRegistry;
use crate::network::types::NodeInfo;

use rinse::{Address, AspectHash, AsyncDestination};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

const NODE_ASPECT_NAME: &str = "nomadnetwork.node";

pub struct NetworkClient {
    registry: Arc<RwLock<NodeRegistry>>,
    known_destinations: Arc<RwLock<HashMap<Address, AsyncDestination>>>,
    node_announce_tx: broadcast::Sender<NodeInfo>,
}

impl NetworkClient {
    pub fn new(registry: NodeRegistry) -> Self {
        let (node_announce_tx, _) = broadcast::channel(64);

        Self {
            registry: Arc::new(RwLock::new(registry)),
            known_destinations: Arc::new(RwLock::new(HashMap::new())),
            node_announce_tx,
        }
    }

    pub fn node_announces(&self) -> broadcast::Receiver<NodeInfo> {
        self.node_announce_tx.subscribe()
    }

    pub async fn handle_destinations_changed(&self, destinations: Vec<AsyncDestination>) {
        let mut known = self.known_destinations.write().await;
        let node_aspect = AspectHash::from_name(NODE_ASPECT_NAME);

        for dest in destinations {
            // Filter by aspect - only accept nomadnetwork.node announces
            if dest.aspect != node_aspect {
                continue;
            }

            let is_new = !known.contains_key(&dest.address);

            if is_new {
                // Only accept nodes with valid NomadNet app_data
                let name = match dest
                    .app_data
                    .as_ref()
                    .and_then(|data| parse_display_name(data))
                {
                    Some(name) => name,
                    None => {
                        // Has correct aspect but no valid name, skip it
                        known.insert(dest.address, dest);
                        continue;
                    }
                };

                let node = NodeInfo {
                    hash: dest.address,
                    name,
                };

                known.insert(dest.address, dest);

                {
                    let mut reg = self.registry.write().await;
                    reg.save(node.clone());
                }

                let _ = self.node_announce_tx.send(node);
            }
        }
    }

    pub async fn registry(&self) -> tokio::sync::RwLockReadGuard<'_, NodeRegistry> {
        self.registry.read().await
    }

    pub async fn registry_mut(&self) -> tokio::sync::RwLockWriteGuard<'_, NodeRegistry> {
        self.registry.write().await
    }
}

impl Clone for NetworkClient {
    fn clone(&self) -> Self {
        Self {
            registry: self.registry.clone(),
            known_destinations: self.known_destinations.clone(),
            node_announce_tx: self.node_announce_tx.clone(),
        }
    }
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

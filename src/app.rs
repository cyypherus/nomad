use rinse::{AsyncNode, ServiceHandle};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::config::{Config, ConfigError, InterfaceConfig};
use crate::identity::{Identity, IdentityError};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct NomadApp {
    config: Config,
    node: Arc<Mutex<AsyncNode>>,
    service: Option<ServiceHandle>,
    dest_hash: [u8; 16],
}

impl NomadApp {
    pub async fn new() -> Result<Self, AppError> {
        let config = Config::load()?;
        let identity = Identity::load_or_generate()?;

        log::info!("Identity loaded");

        let relay_enabled = config.network.relay;
        let mut node = AsyncNode::new(relay_enabled);

        let service = node.add_service("nomadnetwork", &["/page/*"], identity.inner());
        let dest_hash = service.address();

        log::info!("Our address: {}", hex::encode(dest_hash));

        let enabled_interfaces = config.enabled_interfaces();
        if enabled_interfaces.is_empty() {
            log::warn!("No interfaces configured! Add interfaces to config.toml");
        }

        for (name, iface_config) in &enabled_interfaces {
            match iface_config {
                InterfaceConfig::TCPClientInterface {
                    target_host,
                    target_port,
                    ..
                } => {
                    let addr = format!("{}:{}", target_host, target_port);
                    log::info!("Connecting to {} ({})", name, addr);
                    if let Err(e) = node.connect(&addr).await {
                        log::warn!("Failed to connect to {}: {}", addr, e);
                    }
                }
                InterfaceConfig::TCPServerInterface {
                    listen_ip,
                    listen_port,
                    ..
                } => {
                    let addr = format!("{}:{}", listen_ip, listen_port);
                    log::info!("Starting TCP server {} ({})", name, addr);
                    if let Err(e) = node.listen(&addr).await {
                        log::warn!("Failed to listen on {}: {}", addr, e);
                    }
                }
            }
        }

        if !enabled_interfaces.is_empty() {
            service.announce();
            log::info!("Announced on network");
        }

        Ok(Self {
            config,
            node: Arc::new(Mutex::new(node)),
            service: Some(service),
            dest_hash,
        })
    }

    pub fn dest_hash(&self) -> [u8; 16] {
        self.dest_hash
    }

    pub fn take_node(&mut self) -> Arc<Mutex<AsyncNode>> {
        self.node.clone()
    }

    pub fn take_service(&mut self) -> ServiceHandle {
        self.service.take().expect("service already taken")
    }

    pub fn relay_enabled(&self) -> bool {
        self.config.network.relay
    }
}

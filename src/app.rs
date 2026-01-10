use lxmf::LxmfNode;
use reticulum::iface::tcp_client::TcpClient;
use thiserror::Error;

use crate::config::{Config, ConfigError};
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
    node: LxmfNode,
    dest_hash: [u8; 16],
}

impl NomadApp {
    pub async fn new() -> Result<Self, AppError> {
        let config = Config::load()?;
        let identity = Identity::load_or_generate()?;

        log::info!("Identity loaded");

        let mut node = LxmfNode::new(identity.into_inner());
        let dest_hash = node.register_delivery_destination().await;

        log::info!("Our address: {}", hex::encode(dest_hash));

        let iface = &config.network.testnet;
        log::info!("Connecting to {}", iface);

        node.iface_manager()
            .lock()
            .await
            .spawn(TcpClient::new(iface), TcpClient::spawn);

        node.announce().await;
        log::info!("Announced on network");

        Ok(Self {
            config,
            node,
            dest_hash,
        })
    }

    pub async fn run(&mut self) -> Result<(), AppError> {
        log::info!("Starting Nomad...");
        log::info!("Press Ctrl+C to exit");

        let mut announce_rx = self.node.announce_events().await;

        loop {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    log::info!("Shutting down...");
                    break;
                }
                result = announce_rx.recv() => {
                    match result {
                        Ok(event) => {
                            let dest = event.destination.lock().await;
                            let hash = dest.desc.address_hash;
                            log::info!("Announce: {}", hash);
                        }
                        Err(e) => {
                            log::warn!("Announce channel error: {:?}", e);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn dest_hash(&self) -> [u8; 16] {
        self.dest_hash
    }
}

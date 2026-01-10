use lxmf::{
    ConversationInfo, LxMessage, LxmfNode, StorageError, StoredMessage, DESTINATION_LENGTH,
};
use reticulum::iface::tcp_client::TcpClient;
use reticulum::transport::{AnnounceEvent, ReceivedData};
use reticulum::transport::{Transport, TransportConfig};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::config::{Config, ConfigError};
use crate::conversation::{ConversationManager, SqliteStorage};
use crate::identity::{Identity, IdentityError};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("identity error: {0}")]
    Identity(#[from] IdentityError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("lxmf error: {0}")]
    Lxmf(#[from] lxmf::Error),
}

pub struct NomadApp {
    #[allow(dead_code)]
    config: Config,
    transport: Arc<Mutex<Transport>>,
    node: LxmfNode,
    dest_hash: [u8; 16],
    conversations: ConversationManager<SqliteStorage>,
}

impl NomadApp {
    pub async fn new() -> Result<Self, AppError> {
        let config = Config::load()?;
        let identity = Identity::load_or_generate()?;

        log::info!("Identity loaded");

        let transport_config = TransportConfig::new("nomad", identity.inner().inner(), false);
        let transport = Arc::new(Mutex::new(Transport::new(transport_config)));

        let mut node = LxmfNode::new(identity.into_inner(), transport.clone());
        let dest_hash = node.register_delivery_destination().await;

        log::info!("Our address: {}", hex::encode(dest_hash));

        let storage_path = Config::data_dir()?.join("messages.db");
        let storage = Arc::new(SqliteStorage::open(&storage_path)?);
        let conversations = ConversationManager::new(storage.clone(), dest_hash);

        log::info!("Message storage initialized at {:?}", storage_path);

        let iface = &config.network.testnet;
        log::info!("Connecting to {}", iface);

        transport
            .lock()
            .await
            .iface_manager()
            .lock()
            .await
            .spawn(TcpClient::new(iface), TcpClient::spawn);

        node.announce().await;
        log::info!("Announced on network");

        Ok(Self {
            config,
            transport,
            node,
            dest_hash,
            conversations,
        })
    }

    pub fn dest_hash(&self) -> [u8; 16] {
        self.dest_hash
    }

    pub fn testnet_address(&self) -> &str {
        &self.config.network.testnet
    }

    pub async fn announce_events(&self) -> tokio::sync::broadcast::Receiver<AnnounceEvent> {
        self.node.announce_events().await
    }

    pub async fn received_data_events(&self) -> tokio::sync::broadcast::Receiver<ReceivedData> {
        self.node.received_data_events().await
    }

    pub async fn announce(&self) {
        self.node.announce().await;
    }

    pub async fn handle_announce(&mut self, event: &AnnounceEvent) {
        self.node.handle_announce(event).await;
    }

    pub fn handle_received_message(&mut self, data: &ReceivedData) -> Option<LxMessage> {
        if let Some(msg) = self.node.handle_received_data(data) {
            if let Err(e) = self.conversations.handle_incoming(&msg) {
                log::error!("Failed to store incoming message: {}", e);
            }
            Some(msg)
        } else {
            None
        }
    }

    pub async fn send_message(
        &mut self,
        destination: &[u8; DESTINATION_LENGTH],
        content: &str,
    ) -> Result<(), AppError> {
        let mut msg =
            LxMessage::new(*destination, self.dest_hash).with_content(content.as_bytes().to_vec());
        msg.incoming = false;

        self.node.send_message(&mut msg).await?;

        self.conversations.handle_outgoing(&msg)?;

        Ok(())
    }

    pub fn list_conversations(&self) -> Result<Vec<ConversationInfo>, StorageError> {
        self.conversations.list_conversations()
    }

    pub fn get_conversation(
        &self,
        peer: &[u8; DESTINATION_LENGTH],
    ) -> Result<Vec<StoredMessage>, StorageError> {
        self.conversations.get_conversation(peer, None)
    }

    pub fn mark_conversation_read(
        &self,
        peer: &[u8; DESTINATION_LENGTH],
    ) -> Result<(), StorageError> {
        self.conversations.mark_conversation_read(peer)
    }

    pub fn transport(&self) -> &Arc<Mutex<Transport>> {
        &self.transport
    }
}

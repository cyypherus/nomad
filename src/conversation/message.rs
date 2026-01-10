use lxmf::{LxMessage, MessageStorage, StorageError, StoredMessage, DESTINATION_LENGTH};
use std::sync::Arc;

pub struct ConversationManager<S: MessageStorage> {
    storage: Arc<S>,
    our_hash: [u8; DESTINATION_LENGTH],
}

impl<S: MessageStorage> ConversationManager<S> {
    pub fn new(storage: Arc<S>, our_hash: [u8; DESTINATION_LENGTH]) -> Self {
        Self { storage, our_hash }
    }

    pub fn handle_incoming(&self, msg: &LxMessage) -> Result<(), StorageError> {
        self.storage.store_message(msg, &self.our_hash)
    }

    pub fn handle_outgoing(&self, msg: &LxMessage) -> Result<(), StorageError> {
        self.storage.store_message(msg, &self.our_hash)
    }

    pub fn get_conversation(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
        limit: Option<usize>,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        self.storage.get_conversation(peer_hash, limit, None)
    }

    pub fn get_conversation_before(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
        before_timestamp: f64,
        limit: Option<usize>,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        self.storage
            .get_conversation(peer_hash, limit, Some(before_timestamp))
    }

    pub fn mark_read(&self, hash: &[u8; 32]) -> Result<(), StorageError> {
        self.storage.mark_read(hash)
    }

    pub fn mark_conversation_read(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
    ) -> Result<(), StorageError> {
        self.storage.mark_conversation_read(peer_hash)
    }

    pub fn unread_count(&self) -> Result<usize, StorageError> {
        self.storage.unread_count()
    }

    pub fn list_conversations(&self) -> Result<Vec<lxmf::ConversationInfo>, StorageError> {
        self.storage.list_conversations()
    }

    pub fn delete_message(&self, hash: &[u8; 32]) -> Result<(), StorageError> {
        self.storage.delete_message(hash)
    }

    pub fn delete_conversation(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
    ) -> Result<(), StorageError> {
        self.storage.delete_conversation(peer_hash)
    }
}

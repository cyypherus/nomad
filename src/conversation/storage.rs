use lxmf::{
    ConversationInfo, DeliveryMethod, LxMessage, MessageState, MessageStorage, StorageError,
    StoredMessage, DESTINATION_LENGTH,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Mutex;

pub struct SqliteStorage {
    conn: Mutex<Connection>,
}

impl SqliteStorage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let conn = Connection::open(path).map_err(|e| StorageError::Backend(e.to_string()))?;

        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn in_memory() -> Result<Self, StorageError> {
        let conn =
            Connection::open_in_memory().map_err(|e| StorageError::Backend(e.to_string()))?;

        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                hash BLOB PRIMARY KEY,
                destination_hash BLOB NOT NULL,
                source_hash BLOB NOT NULL,
                timestamp REAL NOT NULL,
                title BLOB,
                content BLOB,
                state INTEGER NOT NULL,
                method INTEGER NOT NULL,
                incoming INTEGER NOT NULL,
                signature_validated INTEGER NOT NULL,
                read INTEGER NOT NULL DEFAULT 0,
                packed BLOB,
                created_at REAL NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_dest ON messages(destination_hash);
            CREATE INDEX IF NOT EXISTS idx_messages_source ON messages(source_hash);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
            CREATE INDEX IF NOT EXISTS idx_messages_read ON messages(read);
            "#,
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    fn row_to_stored_message(row: &rusqlite::Row) -> rusqlite::Result<StoredMessage> {
        let hash_vec: Vec<u8> = row.get(0)?;
        let dest_vec: Vec<u8> = row.get(1)?;
        let source_vec: Vec<u8> = row.get(2)?;
        let timestamp: f64 = row.get(3)?;
        let title: Vec<u8> = row.get::<_, Option<Vec<u8>>>(4)?.unwrap_or_default();
        let content: Vec<u8> = row.get::<_, Option<Vec<u8>>>(5)?.unwrap_or_default();
        let state_raw: u8 = row.get(6)?;
        let method_raw: u8 = row.get(7)?;
        let incoming: bool = row.get(8)?;
        let signature_validated: bool = row.get(9)?;
        let read: bool = row.get(10)?;
        let packed: Option<Vec<u8>> = row.get(11)?;

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&hash_vec);

        let mut destination_hash = [0u8; DESTINATION_LENGTH];
        destination_hash.copy_from_slice(&dest_vec);

        let mut source_hash = [0u8; DESTINATION_LENGTH];
        source_hash.copy_from_slice(&source_vec);

        let state = match state_raw {
            0x00 => MessageState::Generating,
            0x01 => MessageState::Outbound,
            0x02 => MessageState::Sending,
            0x04 => MessageState::Sent,
            0x08 => MessageState::Delivered,
            0xFD => MessageState::Rejected,
            0xFE => MessageState::Cancelled,
            _ => MessageState::Failed,
        };

        let method = match method_raw {
            0x01 => DeliveryMethod::Opportunistic,
            0x02 => DeliveryMethod::Direct,
            0x03 => DeliveryMethod::Propagated,
            0x05 => DeliveryMethod::Paper,
            _ => DeliveryMethod::Unknown,
        };

        Ok(StoredMessage {
            hash,
            destination_hash,
            source_hash,
            timestamp,
            title,
            content,
            state,
            method,
            incoming,
            signature_validated,
            read,
            packed,
        })
    }
}

impl MessageStorage for SqliteStorage {
    fn store_message(
        &self,
        msg: &LxMessage,
        _our_hash: &[u8; DESTINATION_LENGTH],
    ) -> Result<(), StorageError> {
        let hash = msg
            .hash
            .ok_or_else(|| StorageError::Backend("message has no hash".into()))?;
        let timestamp = msg
            .timestamp
            .ok_or_else(|| StorageError::Backend("message has no timestamp".into()))?;

        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT OR REPLACE INTO messages 
            (hash, destination_hash, source_hash, timestamp, title, content, state, method, incoming, signature_validated, read, packed, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                hash.as_slice(),
                msg.destination_hash.as_slice(),
                msg.source_hash.as_slice(),
                timestamp,
                if msg.title.is_empty() { None } else { Some(&msg.title) },
                if msg.content.is_empty() { None } else { Some(&msg.content) },
                msg.state as u8,
                msg.method as u8,
                msg.incoming,
                msg.signature_validated,
                false,
                msg.packed_bytes(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64(),
            ],
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;

        Ok(())
    }

    fn get_message(&self, hash: &[u8; 32]) -> Result<StoredMessage, StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT hash, destination_hash, source_hash, timestamp, title, content, state, method, incoming, signature_validated, read, packed FROM messages WHERE hash = ?1",
            params![hash.as_slice()],
            Self::row_to_stored_message,
        )
        .optional()
        .map_err(|e| StorageError::Backend(e.to_string()))?
        .ok_or(StorageError::NotFound)
    }

    fn get_conversation(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
        limit: Option<usize>,
        before_timestamp: Option<f64>,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        let conn = self.conn.lock().unwrap();
        let limit_val = limit.unwrap_or(1000) as i64;
        let before_ts = before_timestamp.unwrap_or(f64::MAX);

        let mut stmt = conn
            .prepare(
                r#"
                SELECT hash, destination_hash, source_hash, timestamp, title, content, state, method, incoming, signature_validated, read, packed
                FROM messages 
                WHERE (destination_hash = ?1 OR source_hash = ?1)
                AND timestamp < ?2
                ORDER BY timestamp ASC
                LIMIT ?3
                "#,
            )
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let rows = stmt
            .query_map(
                params![peer_hash.as_slice(), before_ts, limit_val],
                Self::row_to_stored_message,
            )
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row.map_err(|e| StorageError::Backend(e.to_string()))?);
        }
        Ok(messages)
    }

    fn list_conversations(&self) -> Result<Vec<ConversationInfo>, StorageError> {
        let conn = self.conn.lock().unwrap();

        let mut stmt = conn
            .prepare(
                r#"
                SELECT 
                    CASE WHEN incoming = 1 THEN source_hash ELSE destination_hash END as peer_hash,
                    COUNT(*) as message_count,
                    SUM(CASE WHEN read = 0 THEN 1 ELSE 0 END) as unread_count,
                    MAX(timestamp) as last_timestamp
                FROM messages
                GROUP BY peer_hash
                ORDER BY last_timestamp DESC
                "#,
            )
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let peer_vec: Vec<u8> = row.get(0)?;
                let message_count: i64 = row.get(1)?;
                let unread_count: i64 = row.get(2)?;
                let last_timestamp: Option<f64> = row.get(3)?;

                let mut peer_hash = [0u8; DESTINATION_LENGTH];
                peer_hash.copy_from_slice(&peer_vec);

                Ok(ConversationInfo {
                    peer_hash,
                    message_count: message_count as usize,
                    unread_count: unread_count as usize,
                    last_timestamp,
                    last_message_preview: None,
                })
            })
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        let mut conversations: Vec<ConversationInfo> = Vec::new();
        for row in rows {
            let mut info = row.map_err(|e| StorageError::Backend(e.to_string()))?;

            if let Some(preview) = self.get_last_message_preview(&conn, &info.peer_hash)? {
                info.last_message_preview = Some(preview);
            }
            conversations.push(info);
        }
        Ok(conversations)
    }

    fn mark_read(&self, hash: &[u8; 32]) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute(
                "UPDATE messages SET read = 1 WHERE hash = ?1",
                params![hash.as_slice()],
            )
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if rows == 0 {
            Err(StorageError::NotFound)
        } else {
            Ok(())
        }
    }

    fn mark_conversation_read(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET read = 1 WHERE destination_hash = ?1 OR source_hash = ?1",
            params![peer_hash.as_slice()],
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    fn delete_message(&self, hash: &[u8; 32]) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute(
                "DELETE FROM messages WHERE hash = ?1",
                params![hash.as_slice()],
            )
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if rows == 0 {
            Err(StorageError::NotFound)
        } else {
            Ok(())
        }
    }

    fn delete_conversation(
        &self,
        peer_hash: &[u8; DESTINATION_LENGTH],
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM messages WHERE destination_hash = ?1 OR source_hash = ?1",
            params![peer_hash.as_slice()],
        )
        .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    fn update_message_state(
        &self,
        hash: &[u8; 32],
        state: MessageState,
    ) -> Result<(), StorageError> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute(
                "UPDATE messages SET state = ?1 WHERE hash = ?2",
                params![state as u8, hash.as_slice()],
            )
            .map_err(|e| StorageError::Backend(e.to_string()))?;

        if rows == 0 {
            Err(StorageError::NotFound)
        } else {
            Ok(())
        }
    }

    fn unread_count(&self) -> Result<usize, StorageError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages WHERE read = 0", [], |row| {
                row.get(0)
            })
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(count as usize)
    }

    fn total_message_count(&self) -> Result<usize, StorageError> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(count as usize)
    }
}

impl SqliteStorage {
    fn get_last_message_preview(
        &self,
        conn: &Connection,
        peer_hash: &[u8; DESTINATION_LENGTH],
    ) -> Result<Option<String>, StorageError> {
        let content: Option<Vec<u8>> = conn
            .query_row(
                r#"
                SELECT content FROM messages 
                WHERE destination_hash = ?1 OR source_hash = ?1
                ORDER BY timestamp DESC
                LIMIT 1
                "#,
                params![peer_hash.as_slice()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| StorageError::Backend(e.to_string()))?
            .flatten();

        Ok(content.and_then(|c| {
            String::from_utf8(c).ok().map(|s| {
                if s.len() > 50 {
                    format!("{}...", &s[..47])
                } else {
                    s
                }
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn create_test_message(dest: [u8; 16], source: [u8; 16], content: &str) -> LxMessage {
        let mut rng = rand::thread_rng();
        let signing_key = SigningKey::generate(&mut rng);

        let mut msg = LxMessage::new(dest, source).with_content(content.as_bytes().to_vec());
        msg.pack(&signing_key).unwrap();
        msg.incoming = true;
        msg
    }

    #[test]
    fn test_store_and_retrieve() {
        let storage = SqliteStorage::in_memory().unwrap();
        let dest: [u8; 16] = rand::random();
        let source: [u8; 16] = rand::random();

        let msg = create_test_message(dest, source, "Hello world");
        let hash = msg.hash.unwrap();

        storage.store_message(&msg, &dest).unwrap();

        let retrieved = storage.get_message(&hash).unwrap();
        assert_eq!(retrieved.content_as_string().unwrap(), "Hello world");
        assert_eq!(retrieved.destination_hash, dest);
        assert_eq!(retrieved.source_hash, source);
    }

    #[test]
    fn test_conversation_list() {
        let storage = SqliteStorage::in_memory().unwrap();
        let our_hash: [u8; 16] = rand::random();
        let peer1: [u8; 16] = rand::random();
        let peer2: [u8; 16] = rand::random();

        let msg1 = create_test_message(our_hash, peer1, "From peer1");
        let msg2 = create_test_message(our_hash, peer2, "From peer2");

        storage.store_message(&msg1, &our_hash).unwrap();
        storage.store_message(&msg2, &our_hash).unwrap();

        let convos = storage.list_conversations().unwrap();
        assert_eq!(convos.len(), 2);
    }

    #[test]
    fn test_mark_read() {
        let storage = SqliteStorage::in_memory().unwrap();
        let dest: [u8; 16] = rand::random();
        let source: [u8; 16] = rand::random();

        let msg = create_test_message(dest, source, "Unread message");
        let hash = msg.hash.unwrap();

        storage.store_message(&msg, &dest).unwrap();
        assert_eq!(storage.unread_count().unwrap(), 1);

        storage.mark_read(&hash).unwrap();
        assert_eq!(storage.unread_count().unwrap(), 0);

        let retrieved = storage.get_message(&hash).unwrap();
        assert!(retrieved.read);
    }

    #[test]
    fn test_delete_conversation() {
        let storage = SqliteStorage::in_memory().unwrap();
        let our_hash: [u8; 16] = rand::random();
        let peer: [u8; 16] = rand::random();

        let msg1 = create_test_message(our_hash, peer, "Message 1");
        let msg2 = create_test_message(our_hash, peer, "Message 2");

        storage.store_message(&msg1, &our_hash).unwrap();
        storage.store_message(&msg2, &our_hash).unwrap();

        assert_eq!(storage.total_message_count().unwrap(), 2);

        storage.delete_conversation(&peer).unwrap();
        assert_eq!(storage.total_message_count().unwrap(), 0);
    }
}

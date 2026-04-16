use crate::{ConxianError, ConxianResult, PersistentState, Persistence};
use std::fs;
use std::path::PathBuf;

pub struct FilePersistence {
    path: PathBuf,
}

impl FilePersistence {
    pub fn new(path: &str) -> Self {
        Self {
            path: PathBuf::from(path),
        }
    }
}

impl Persistence for FilePersistence {
    fn save(&self, state: &PersistentState) -> ConxianResult<()> {
        let json = serde_json::to_string(state)
            .map_err(|e| ConxianError::Io(format!("Serialization failed: {}", e)))?;

        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, json)
            .map_err(|e| ConxianError::Io(format!("Write to temporary file failed: {}", e)))?;

        fs::rename(&tmp_path, &self.path)
            .map_err(|e| ConxianError::Io(format!("Atomic rename failed: {}", e)))?;

        Ok(())
    }

    fn load(&self) -> ConxianResult<PersistentState> {
        if !self.path.exists() {
            return Ok(PersistentState::default());
        }

        let content = fs::read_to_string(&self.path)
            .map_err(|e| ConxianError::Io(format!("Read failed: {}", e)))?;

        let state: PersistentState = serde_json::from_str(&content)
            .map_err(|e| ConxianError::Io(format!("Deserialization failed: {}", e)))?;

        Ok(state)
    }
}

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rusqlite::{params, Connection};
use std::sync::Mutex;

pub struct EncryptedOfflineQueue {
    conn: Mutex<Connection>,
    encryption_key: [u8; 32],
}

impl EncryptedOfflineQueue {
    pub fn new(path: &str, key: [u8; 32]) -> ConxianResult<Self> {
        let conn = Connection::open(path)
            .map_err(|e| ConxianError::Io(format!("Failed to open SQLite: {}", e)))?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS offline_receipts (
                id TEXT PRIMARY KEY,
                encrypted_payload BLOB NOT NULL,
                nonce BLOB NOT NULL,
                status TEXT NOT NULL,
                timestamp INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| ConxianError::Io(format!("Failed to create table: {}", e)))?;

        Ok(Self {
            conn: Mutex::new(conn),
            encryption_key: key,
        })
    }

    fn encrypt(&self, data: &[u8]) -> ConxianResult<(Vec<u8>, [u8; 12])> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| ConxianError::Security(format!("Cipher init failed: {}", e)))?;

        let mut nonce_bytes = [0u8; 12];
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let now_bytes = now.to_le_bytes();
        nonce_bytes.copy_from_slice(&now_bytes[..12]);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, data)
            .map_err(|e| ConxianError::Security(format!("Encryption failed: {}", e)))?;

        Ok((ciphertext, nonce_bytes))
    }

    fn decrypt(&self, ciphertext: &[u8], nonce_bytes: &[u8]) -> ConxianResult<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| ConxianError::Security(format!("Cipher init failed: {}", e)))?;
        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| ConxianError::Security(format!("Decryption failed: {}", e)))
    }
}

impl crate::OfflineQueue for EncryptedOfflineQueue {
    fn enqueue(&self, receipt: &crate::OfflineReceipt) -> ConxianResult<()> {
        let json = serde_json::to_vec(receipt)
            .map_err(|e| ConxianError::Io(format!("Serialization failed: {}", e)))?;

        let (encrypted, nonce) = self.encrypt(&json)?;
        let status = format!("{:?}", receipt.status);

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO offline_receipts (id, encrypted_payload, nonce, status, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![receipt.receipt_id, encrypted, nonce.to_vec(), status, receipt.timestamp],
        ).map_err(|e| ConxianError::Io(format!("Enqueue failed: {}", e)))?;

        Ok(())
    }

    fn dequeue_pending(&self) -> ConxianResult<Vec<crate::OfflineReceipt>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT encrypted_payload, nonce FROM offline_receipts WHERE UPPER(status) = 'PENDING' OR UPPER(status) = 'GOSSIPED'")
            .map_err(|e| ConxianError::Io(format!("Prepare failed: {}", e)))?;

        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .map_err(|e| ConxianError::Io(format!("Query failed: {}", e)))?;

        let mut receipts = Vec::new();
        for row in rows {
            let (encrypted, nonce): (Vec<u8>, Vec<u8>) =
                row.map_err(|e| ConxianError::Io(format!("Row error: {}", e)))?;
            let decrypted = self.decrypt(&encrypted, &nonce)?;
            let receipt: crate::OfflineReceipt = serde_json::from_slice(&decrypted)
                .map_err(|e| ConxianError::Io(format!("Deserialization failed: {}", e)))?;
            receipts.push(receipt);
        }

        Ok(receipts)
    }

    fn mark_broadcasted(&self, receipt_id: &str) -> ConxianResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE offline_receipts SET status = 'BROADCASTED' WHERE id = ?1",
            params![receipt_id],
        )
        .map_err(|e| ConxianError::Io(format!("Update failed: {}", e)))?;
        Ok(())
    }
}

use crate::{ConxianError, ConxianResult, Persistence, PersistentState, VersionedPersistentState};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const STATE_FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateEnvelope {
    format_version: u32,
    revision: u64,
    state: PersistentState,
}

pub struct FilePersistence {
    path: PathBuf,
    lock_path: PathBuf,
}

impl FilePersistence {
    pub fn new(path: &str) -> Self {
        let path = PathBuf::from(path);
        Self {
            lock_path: path.with_extension("transaction.lock"),
            path,
        }
    }

    fn with_lock<T>(&self, operation: impl FnOnce() -> ConxianResult<T>) -> ConxianResult<T> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(|error| ConxianError::Persistence(error.to_string()))?;
        lock.lock_exclusive()
            .map_err(|error| ConxianError::Persistence(error.to_string()))?;
        let result = operation();
        let unlock =
            FileExt::unlock(&lock).map_err(|error| ConxianError::Persistence(error.to_string()));
        match (result, unlock) {
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
            (Ok(value), Ok(())) => Ok(value),
        }
    }

    fn read_unlocked(&self) -> ConxianResult<VersionedPersistentState> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(VersionedPersistentState {
                    revision: 0,
                    state: PersistentState::default(),
                });
            }
            Err(error) => return Err(ConxianError::Persistence(error.to_string())),
        };
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| ConxianError::Persistence(error.to_string()))?;
        if value.get("format_version").is_some()
            || value.get("revision").is_some()
            || value.get("state").is_some()
        {
            let envelope: StateEnvelope = serde_json::from_value(value)
                .map_err(|error| ConxianError::Persistence(error.to_string()))?;
            if envelope.format_version != STATE_FORMAT_VERSION {
                return Err(ConxianError::Persistence(format!(
                    "unsupported persistence format version {}",
                    envelope.format_version
                )));
            }
            Ok(VersionedPersistentState {
                revision: envelope.revision,
                state: envelope.state,
            })
        } else {
            let state = serde_json::from_value(value)
                .map_err(|error| ConxianError::Persistence(error.to_string()))?;
            Ok(VersionedPersistentState { revision: 0, state })
        }
    }
}

impl Persistence for FilePersistence {
    fn load_versioned(&self) -> ConxianResult<VersionedPersistentState> {
        self.with_lock(|| self.read_unlocked())
    }

    fn compare_and_swap(
        &self,
        expected_revision: u64,
        new_state: &PersistentState,
    ) -> ConxianResult<VersionedPersistentState> {
        self.with_lock(|| {
            let current = self.read_unlocked()?;
            if current.revision != expected_revision {
                return Err(ConxianError::PersistenceConflict {
                    expected: expected_revision,
                    actual: current.revision,
                });
            }
            let revision = current
                .revision
                .checked_add(1)
                .ok_or_else(|| ConxianError::Persistence("revision overflow".to_string()))?;
            let envelope = StateEnvelope {
                format_version: STATE_FORMAT_VERSION,
                revision,
                state: new_state.clone(),
            };
            let bytes = serde_json::to_vec_pretty(&envelope)
                .map_err(|error| ConxianError::Persistence(error.to_string()))?;
            let temporary = self
                .path
                .with_extension(format!("tmp-{}", std::process::id()));
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)
                .map_err(|error| ConxianError::Persistence(error.to_string()))?;
            let write_result = (|| {
                file.write_all(&bytes)
                    .map_err(|error| ConxianError::Persistence(error.to_string()))?;
                file.sync_all()
                    .map_err(|error| ConxianError::Persistence(error.to_string()))?;
                drop(file);
                fs::rename(&temporary, &self.path)
                    .map_err(|error| ConxianError::Persistence(error.to_string()))?;
                if let Some(parent) = self.path.parent() {
                    File::open(parent)
                        .and_then(|directory| directory.sync_all())
                        .map_err(|error| ConxianError::PersistenceCommitUnknown {
                            revision,
                            message: error.to_string(),
                        })?;
                }
                Ok(())
            })();
            if write_result.is_err() {
                let _ = fs::remove_file(&temporary);
            }
            write_result?;
            Ok(VersionedPersistentState {
                revision,
                state: new_state.clone(),
            })
        })
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

        conn.execute(
            "CREATE TABLE IF NOT EXISTS webhook_replay_keys (
                replay_key TEXT PRIMARY KEY,
                expires_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )
        .map_err(|e| ConxianError::Io(format!("Failed to create replay table: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_webhook_replay_keys_expires_at
             ON webhook_replay_keys (expires_at)",
            [],
        )
        .map_err(|e| ConxianError::Io(format!("Failed to create replay index: {}", e)))?;

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
        let nonce = Nonce::try_from(&nonce_bytes[..])
            .map_err(|e| ConxianError::Security(format!("Invalid nonce length: {}", e)))?;

        let ciphertext = cipher
            .encrypt(&nonce, data)
            .map_err(|e| ConxianError::Security(format!("Encryption failed: {}", e)))?;

        Ok((ciphertext, nonce_bytes))
    }

    fn decrypt(&self, ciphertext: &[u8], nonce_bytes: &[u8]) -> ConxianResult<Vec<u8>> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|e| ConxianError::Security(format!("Cipher init failed: {}", e)))?;
        let nonce = Nonce::try_from(nonce_bytes)
            .map_err(|e| ConxianError::Security(format!("Invalid nonce length: {}", e)))?;

        cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|e| ConxianError::Security(format!("Decryption failed: {}", e)))
    }
}

impl crate::OfflineQueue for EncryptedOfflineQueue {
    fn enqueue(&self, receipt: &crate::OfflineReceipt) -> ConxianResult<()> {
        let json = serde_json::to_vec(receipt)
            .map_err(|e| ConxianError::Io(format!("Serialization failed: {}", e)))?;

        let (encrypted, nonce) = self.encrypt(&json)?;
        let status = format!("{:?}", receipt.status);
        let timestamp = i64::try_from(receipt.timestamp)
            .map_err(|_| ConxianError::Io("Timestamp overflow".to_string()))?;

        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "INSERT INTO offline_receipts (id, encrypted_payload, nonce, status, timestamp) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![receipt.receipt_id, encrypted, nonce.to_vec(), status, timestamp],
        ).map_err(|e| ConxianError::Io(format!("Enqueue failed: {}", e)))?;

        Ok(())
    }

    fn dequeue_pending(&self) -> ConxianResult<Vec<crate::OfflineReceipt>> {
        let conn = self.conn.lock().expect("lock poisoned");
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
        let conn = self.conn.lock().expect("lock poisoned");
        conn.execute(
            "UPDATE offline_receipts SET status = 'BROADCASTED' WHERE id = ?1",
            params![receipt_id],
        )
        .map_err(|e| ConxianError::Io(format!("Update failed: {}", e)))?;
        Ok(())
    }

    fn claim_replay_key(&self, replay_key: &str, ttl_seconds: u64) -> ConxianResult<bool> {
        if replay_key.trim().is_empty() {
            return Err(ConxianError::Persistence(
                "Replay key cannot be empty".to_string(),
            ));
        }

        if ttl_seconds == 0 {
            return Err(ConxianError::Persistence(
                "Replay key TTL must be greater than zero".to_string(),
            ));
        }

        let now_seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| ConxianError::Persistence(format!("Clock error: {}", e)))?
            .as_secs();

        let expires_at = now_seconds
            .checked_add(ttl_seconds)
            .ok_or_else(|| ConxianError::Persistence("Replay key TTL overflow".to_string()))?;

        let now_i64 = i64::try_from(now_seconds)
            .map_err(|_| ConxianError::Persistence("Timestamp overflow".to_string()))?;
        let expires_at_i64 = i64::try_from(expires_at)
            .map_err(|_| ConxianError::Persistence("Expiry timestamp overflow".to_string()))?;

        let conn = self.conn.lock().expect("lock poisoned");

        conn.execute(
            "DELETE FROM webhook_replay_keys WHERE expires_at < ?1",
            params![now_i64],
        )
        .map_err(|e| ConxianError::Persistence(format!("Replay cleanup failed: {}", e)))?;

        let affected = conn
            .execute(
                "INSERT INTO webhook_replay_keys (replay_key, expires_at, created_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(replay_key) DO UPDATE SET
                    expires_at = excluded.expires_at,
                    created_at = excluded.created_at
                 WHERE webhook_replay_keys.expires_at < excluded.created_at",
                params![replay_key, expires_at_i64, now_i64],
            )
            .map_err(|e| ConxianError::Persistence(format!("Replay claim failed: {}", e)))?;

        Ok(affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::EncryptedOfflineQueue;
    use crate::OfflineQueue;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn tmp_db_path(prefix: &str) -> String {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock moved backwards")
            .as_nanos();
        format!("{}_{}.db", prefix, suffix)
    }

    #[test]
    fn claim_replay_key_rejects_duplicate_until_expiry() {
        let db_path = tmp_db_path("replay_claim");
        let queue = EncryptedOfflineQueue::new(&db_path, [7u8; 32]).unwrap();

        assert!(queue.claim_replay_key("ramp:sig:hash", 1).unwrap());
        assert!(!queue.claim_replay_key("ramp:sig:hash", 1).unwrap());

        std::thread::sleep(Duration::from_secs(2));
        assert!(queue.claim_replay_key("ramp:sig:hash", 1).unwrap());

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn claim_replay_key_allows_distinct_keys() {
        let db_path = tmp_db_path("replay_distinct");
        let queue = EncryptedOfflineQueue::new(&db_path, [3u8; 32]).unwrap();

        assert!(queue.claim_replay_key("ramp:sig1:hash", 60).unwrap());
        assert!(queue.claim_replay_key("ramp:sig2:hash", 60).unwrap());

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn decrypt_rejects_invalid_nonce_length_as_security_error() {
        let queue = EncryptedOfflineQueue::new(":memory:", [5u8; 32]).unwrap();

        let error = queue.decrypt(b"ciphertext", &[0u8; 11]).unwrap_err();

        assert!(matches!(
            error,
            crate::ConxianError::Security(message)
                if message.starts_with("Invalid nonce length:")
        ));
    }
}

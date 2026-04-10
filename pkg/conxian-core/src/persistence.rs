use crate::{ConxianError, ConxianResult, Persistence, PersistentState};
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

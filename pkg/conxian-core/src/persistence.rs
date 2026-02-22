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
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| ConxianError::Internal(e.to_string()))?;

        let mut temp_path = self.path.clone();
        temp_path.set_extension("tmp");

        fs::write(&temp_path, json).map_err(|e| ConxianError::Io(e.to_string()))?;
        fs::rename(&temp_path, &self.path).map_err(|e| ConxianError::Io(e.to_string()))?;
        Ok(())
    }

    fn load(&self) -> ConxianResult<PersistentState> {
        if !self.path.exists() {
            return Ok(PersistentState::default());
        }
        let json = fs::read_to_string(&self.path).map_err(|e| ConxianError::Io(e.to_string()))?;
        let state: PersistentState =
            serde_json::from_str(&json).map_err(|e| ConxianError::Internal(e.to_string()))?;
        Ok(state)
    }
}

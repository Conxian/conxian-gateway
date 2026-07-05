use conxian_core::{ConxianResult, Persistence, PersistentState};
use std::fs;
use std::io::{Read, Write};
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
            .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?;
        let mut file = fs::File::create(&self.path)
            .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?;
        file.write_all(json.as_bytes())
            .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?;
        Ok(())
    }

    fn load(&self) -> ConxianResult<PersistentState> {
        if !self.path.exists() {
            return Ok(PersistentState::default());
        }
        let mut file = fs::File::open(&self.path)
            .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?;
        let mut json = String::new();
        file.read_to_string(&mut json)
            .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?;
        let state: PersistentState = serde_json::from_str(&json)
            .map_err(|e| conxian_core::ConxianError::Internal(e.to_string()))?;
        Ok(state)
    }
}

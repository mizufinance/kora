use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::DkgError;

#[derive(Debug, Clone)]
pub struct DkgOutput {
    pub group_public_key: Vec<u8>,
    pub public_polynomial: Vec<u8>,
    pub threshold: u32,
    pub participants: usize,
    pub share_index: u32,
    pub share_secret: Vec<u8>,
    pub participant_keys: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
struct OutputJson {
    group_public_key: String,
    public_polynomial: String,
    threshold: u32,
    participants: usize,
    #[serde(default)]
    participant_keys: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ShareJson {
    index: u32,
    secret: String,
}

impl DkgOutput {
    pub fn save(&self, data_dir: &Path) -> Result<(), DkgError> {
        let output_json = OutputJson {
            group_public_key: hex::encode(&self.group_public_key),
            public_polynomial: hex::encode(&self.public_polynomial),
            threshold: self.threshold,
            participants: self.participants,
            participant_keys: self.participant_keys.iter().map(hex::encode).collect(),
        };

        let output_path = data_dir.join("output.json");
        std::fs::write(&output_path, serde_json::to_string_pretty(&output_json)?)?;

        let share_json =
            ShareJson { index: self.share_index, secret: hex::encode(&self.share_secret) };

        let share_path = data_dir.join("share.key");
        std::fs::write(&share_path, serde_json::to_string_pretty(&share_json)?)?;

        Ok(())
    }

    pub fn load(data_dir: &Path) -> Result<Self, DkgError> {
        let output_path = data_dir.join("output.json");
        let output_str = std::fs::read_to_string(&output_path)?;
        let output: OutputJson = serde_json::from_str(&output_str)
            .map_err(|e| DkgError::Serialization(e.to_string()))?;

        let share_path = data_dir.join("share.key");
        let share_str = std::fs::read_to_string(&share_path)?;
        let share: ShareJson =
            serde_json::from_str(&share_str).map_err(|e| DkgError::Serialization(e.to_string()))?;

        let participant_keys = output
            .participant_keys
            .iter()
            .map(|k| hex::decode(k).map_err(|e| DkgError::Serialization(e.to_string())))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self {
            group_public_key: hex::decode(&output.group_public_key)
                .map_err(|e| DkgError::Serialization(e.to_string()))?,
            public_polynomial: hex::decode(&output.public_polynomial)
                .map_err(|e| DkgError::Serialization(e.to_string()))?,
            threshold: output.threshold,
            participants: output.participants,
            share_index: share.index,
            share_secret: hex::decode(&share.secret)
                .map_err(|e| DkgError::Serialization(e.to_string()))?,
            participant_keys,
        })
    }

    pub fn exists(data_dir: &Path) -> bool {
        data_dir.join("output.json").exists() && data_dir.join("share.key").exists()
    }
}

impl From<serde_json::Error> for DkgError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serialization(e.to_string())
    }
}

use std::sync::Arc;

use commonware_codec::Write as _;
use commonware_cryptography::bls12381::{
    dkg,
    primitives::{sharing::Mode, variant::MinSig},
};
use commonware_utils::{N3f1, TryCollect, ordered::Set};
use tokio::sync::Mutex;
use tracing::info;

use crate::{config::DkgConfig, error::DkgError, output::DkgOutput, state::DkgState};

pub struct DkgCeremony {
    config: DkgConfig,
    #[allow(dead_code)]
    state: Arc<Mutex<DkgState>>,
}

impl DkgCeremony {
    pub fn new(config: DkgConfig) -> Self {
        Self {
            config,
            state: Arc::new(Mutex::new(DkgState::new(0))),
        }
    }

    pub async fn run(&self) -> Result<DkgOutput, DkgError> {
        info!(
            validator_index = self.config.validator_index,
            n = self.config.n(),
            t = self.config.t(),
            "Starting DKG ceremony"
        );

        if DkgOutput::exists(&self.config.data_dir) {
            info!("DKG output already exists, loading from disk");
            return DkgOutput::load(&self.config.data_dir);
        }

        let participants_set: Set<_> = self
            .config
            .participants
            .iter()
            .cloned()
            .try_collect()
            .map_err(|_| DkgError::CeremonyFailed("duplicate participants".into()))?;

        let mut rng = rand::rngs::OsRng;

        info!("Running local DKG deal");
        let (public_output, shares) =
            dkg::deal::<MinSig, _, N3f1>(&mut rng, Mode::default(), participants_set)
                .map_err(|e| DkgError::Crypto(format!("DKG deal failed: {:?}", e)))?;

        let my_pk = self.config.my_public_key();
        let my_share = shares
            .get_value(&my_pk)
            .ok_or(DkgError::MissingShare)?
            .clone();

        info!("DKG ceremony completed successfully");

        let group_key = public_output.public();
        let mut group_key_bytes = Vec::new();
        commonware_codec::Write::write(group_key, &mut group_key_bytes);

        let mut share_bytes = Vec::new();
        commonware_codec::Write::write(&my_share, &mut share_bytes);

        let participant_keys: Vec<Vec<u8>> = self
            .config
            .participants
            .iter()
            .map(|pk| {
                let mut bytes = Vec::new();
                commonware_codec::Write::write(pk, &mut bytes);
                bytes
            })
            .collect();

        // Serialize the full polynomial (Sharing) for reconstruction
        let mut polynomial_bytes = Vec::new();
        public_output.public().write(&mut polynomial_bytes);

        let output = DkgOutput {
            group_public_key: group_key_bytes,
            public_polynomial: polynomial_bytes,
            threshold: self.config.t() as u32,
            participants: self.config.n(),
            share_index: self.config.validator_index as u32,
            share_secret: share_bytes,
            participant_keys,
        };

        output.save(&self.config.data_dir)?;
        info!(path = ?self.config.data_dir, "Saved DKG output");

        Ok(output)
    }
}

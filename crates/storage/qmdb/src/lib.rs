#![doc = include_str!("../README.md")]
#![doc(issue_tracker_base_url = "https://github.com/anthropics/kora/issues/")]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]
#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod database;
pub use database::QmdbDatabase;

mod encoding;
pub use encoding::{StorageKey, decode_account, encode_account};

mod error;
pub use error::QmdbError;

mod traits;
pub use traits::{QmdbBatchable, QmdbGettable};

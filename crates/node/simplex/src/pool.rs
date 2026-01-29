//! Provides a default buffer pool implementation.

use commonware_runtime::buffer::PoolRef;
use commonware_utils::{NZU16, NZUsize};

/// Default page size in bytes (32 KiB).
///
/// This matches the minimum floor required by consensus message serialization
/// (BLS signatures, finalization certificates, etc.).
pub const DEFAULT_PAGE_SIZE: u16 = 32_768;

/// Default pool capacity (number of pages).
pub const DEFAULT_POOL_CAPACITY: usize = 10_000;

/// The default buffer pool constructor.
#[derive(Debug, Clone, Copy)]
pub struct DefaultPool;

impl DefaultPool {
    /// Initializes a default [`PoolRef`].
    ///
    /// Uses a page size of 16 KiB and a capacity of 10,000 pages.
    pub fn init() -> PoolRef {
        PoolRef::new(NZU16!(DEFAULT_PAGE_SIZE), NZUsize!(DEFAULT_POOL_CAPACITY))
    }
}

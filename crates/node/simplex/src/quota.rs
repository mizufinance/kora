//! Provides a default quota implementation.

use commonware_runtime::Quota;
use commonware_utils::NZU32;

/// Default requests per second.
pub const DEFAULT_REQUESTS_PER_SECOND: u32 = 1_000;

/// The default quota constructor.
#[derive(Debug, Clone, Copy)]
pub struct DefaultQuota;

impl DefaultQuota {
    /// Initializes a default [`Quota`].
    ///
    /// Uses 1,000 requests per second.
    pub const fn init() -> Quota {
        Quota::per_second(NZU32!(DEFAULT_REQUESTS_PER_SECOND))
    }
}

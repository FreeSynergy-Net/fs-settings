//! Where the person service gets the instance layer. v1: nowhere (c0 decision 5); S7: the client of the instance service.

use crate::ports::{Instance, Stamp};

/// The instance layer as the person service sees it.
pub trait InstanceSource: Send + Sync {
    /// The instance's defaults and locks now.
    fn instance(&self) -> Instance;
    /// Moves when the instance layer changes.
    fn stamp(&self) -> Stamp;
}

/// No instance layer: empty (c0 decision 5, until S7).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoInstance;

impl InstanceSource for NoInstance {
    fn instance(&self) -> Instance {
        Instance::default()
    }

    fn stamp(&self) -> Stamp {
        Stamp(0)
    }
}

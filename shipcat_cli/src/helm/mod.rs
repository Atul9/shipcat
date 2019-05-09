/// Need basic manifest handling
pub use super::Manifest;
/// Verify trait gets the Config
pub use super::{AuditWebhook, Config, Region, VersionScheme};
/// Allow normal error handling from structs
pub use super::{Error, ErrorKind, Result, ResultExt};

/// For slack hookback
pub use super::structs::Metadata;

// allow using some slack and kube stuff
pub use super::audit;
pub use super::grafana;
pub use super::kube;
pub use super::slack;

/// Parallel helm invokers
pub mod parallel;

/// Direct helm invokers (used by abstractions)
pub mod direct;
// Re-exports for main
pub use self::direct::{history, status, template, values};

/// Helm related helpers
pub mod helpers;
// Commonly used helper
pub use self::helpers::infer_fallback_version;

pub use self::direct::{UpgradeData, UpgradeMode};

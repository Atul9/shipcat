use super::{Container};
use super::autoscaling::AutoScaling;

/// Worker for a service
///
/// Essentially a side-car like object that can scale resources separately to the main pods.
/// Useful for services that have one single side service that polls or does some work.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Worker {
    /// Replication limits
    pub replicaCount: u32,

    /// Autoscaling parameters
    ///
    /// Overrides the replicaCount for this worker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoScaling: Option<AutoScaling>,

    /// Http Port to expose
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub httpPort: Option<u32>,

    /// Common properties for all types of container
    #[serde(flatten)]
    pub container: Container,
}

use merge::Merge;

use shipcat_definitions::structs::autoscaling::AutoScaling;
use shipcat_definitions::structs::Worker;
use shipcat_definitions::Result;

use super::container::{ContainerBuildParams, ContainerSource};
use crate::util::{Build, Require};

#[derive(Deserialize, Merge, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct WorkerSource {
    pub replica_count: Option<u32>,
    pub auto_scaling: Option<AutoScaling>,
    pub http_port: Option<u32>,

    #[serde(flatten)]
    pub container: ContainerSource,
}

impl Build<Worker, ContainerBuildParams> for WorkerSource {
    fn build(self, params: &ContainerBuildParams) -> Result<Worker> {
        if let Some(a) = &self.auto_scaling {
            a.verify()?;
        }
        Ok(Worker {
            container: self.container.build(params)?,
            replicaCount: self.replica_count.require("replicaCount")?,
            autoScaling: self.auto_scaling,
            httpPort: self.http_port,
        })
    }
}

use merge::Merge;

use shipcat_definitions::structs::job::{JobVolumeClaim, RestartPolicy};
use shipcat_definitions::structs::Job;
use shipcat_definitions::Result;

use crate::util::Build;

use super::container::{ContainerBuildParams, ContainerSource};

#[derive(Deserialize, Merge, Clone, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct JobSource {
    pub volume_claim: Option<JobVolumeClaim>,
    pub timeout: Option<u32>,
    pub restart_policy: Option<RestartPolicy>,

    #[serde(flatten)]
    pub container: ContainerSource,
}

impl Build<Job, ContainerBuildParams> for JobSource {
    fn build(self, params: &ContainerBuildParams) -> Result<Job> {
        let container = self.container.build(params)?;
        match (&container.image, &container.version) {
            (Some(_), None) => bail!("Cannot specify image without specifying version in CronJob"),
            (None, Some(_)) => {
                bail!("Cannot specify the version without specifying an image in CronJob")
            }
            (image, version) => (image, version),
        };
        Ok(Job {
            container,
            volumeClaim: self.volume_claim,
            timeout: self.timeout,
            restartPolicy: self.restart_policy.unwrap_or_default(),
        })
    }
}

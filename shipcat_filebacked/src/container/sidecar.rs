use shipcat_definitions::structs::Container;
use shipcat_definitions::Result;

use super::container::{ContainerBuildParams, ContainerSource};
use crate::util::Build;

#[derive(Deserialize, Clone, Default)]
pub struct SidecarSource(ContainerSource);

impl Build<Container, ContainerBuildParams> for SidecarSource {
    fn build(self, params: &ContainerBuildParams) -> Result<Container> {
        self.0.build(params)
    }
}

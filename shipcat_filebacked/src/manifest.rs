use merge::Merge;
use std::collections::BTreeMap;

use shipcat_definitions::structs::{
    autoscaling::AutoScaling, security::DataHandling, tolerations::Tolerations, volume::Volume,
    ConfigMap, CronJob, Dependency, EnvVars, Gate, HealthCheck, HostAlias, InitContainer, Job,
    Kafka, Kong, LifeCycle, Metadata, PersistentVolume, Port, Probe, Rbac, Resources,
    RollingUpdate, Sidecar, VaultOpts, VolumeMount, Worker,
};
use shipcat_definitions::{Config, Manifest, BaseManifest, Region, Result};
use shipcat_definitions::relaxed_string::{RelaxedString};

use super::{SimpleManifest};

/// Main manifest, deserialized from `shipcat.yml`.
#[derive(Deserialize, Default, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct ManifestSource {
    pub name: Option<String>,
    pub external: bool,
    pub disabled: bool,
    pub regions: Vec<String>,
    pub metadata: Option<Metadata>,

    #[serde(flatten)]
    pub overrides: ManifestOverrides,
}

/// Manifest overrides, deserialized from `dev-uk.yml`/`prod.yml` etc.
#[derive(Deserialize, Default, Merge, Clone)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct ManifestOverrides {
    pub publicly_accessible: Option<bool>,
    pub image: Option<String>,
    pub image_size: Option<u32>,
    pub version: Option<String>,
    pub command: Option<Vec<String>>,
    pub data_handling: Option<DataHandling>,
    pub language: Option<String>,
    pub resources: Option<Resources<RelaxedString>>,
    pub secret_files: BTreeMap<String, String>,
    pub configs: Option<ConfigMap>,
    pub vault: Option<VaultOpts>,
    pub http_port: Option<u32>,
    pub ports: Option<Vec<Port>>,
    pub external_port: Option<u32>,
    pub health: Option<HealthCheck>,
    pub dependencies: Option<Vec<Dependency>>,
    pub workers: Option<Vec<Worker>>,
    pub sidecars: Option<Vec<Sidecar>>,
    pub readiness_probe: Option<Probe>,
    pub liveness_probe: Option<Probe>,
    pub lifecycle: Option<LifeCycle>,
    pub rolling_update: Option<RollingUpdate>,
    pub auto_scaling: Option<AutoScaling>,
    pub tolerations: Option<Vec<Tolerations>>,
    pub host_aliases: Option<Vec<HostAlias>>,
    pub init_containers: Option<Vec<InitContainer>>,
    pub volumes: Option<Vec<Volume>>,
    pub volume_mounts: Option<Vec<VolumeMount>>,
    pub persistent_volumes: Option<Vec<PersistentVolume>>,
    pub cron_jobs: Option<Vec<CronJob>>,
    pub jobs: Option<Vec<Job>>,
    pub service_annotations: BTreeMap<String, String>,
    pub labels: BTreeMap<String, RelaxedString>,
    pub kong: Option<Kong>,
    pub gate: Option<Gate>,
    pub hosts: Option<Vec<String>>,
    pub kafka: Option<Kafka>,
    pub source_ranges: Option<Vec<String>>,
    pub rbac: Option<Vec<Rbac>>,

    #[serde(flatten)]
    pub defaults: ManifestDefaults,
}

/// Global/regional manifest defaults, deserialized from `shipcat.conf` etc.
#[derive(Deserialize, Default, Merge, Clone)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct ManifestDefaults {
    pub image_prefix: Option<String>,
    pub chart: Option<String>,
    pub replica_count: Option<u32>,
    pub env: BTreeMap<String, RelaxedString>,
}

impl ManifestSource {
    /// Build a Manifest from a ManifestSource, validating and mutating properties.
    pub fn build(self, conf: &Config, reg: &Region) -> Result<Manifest> {
        let simple = self.build_simple(conf, reg)?;
        let name = simple.base.name.clone();
        let namespace = self.coalesce_namespace(conf, reg, simple.base.metadata.team.clone());
        let data_handling = self.build_data_handling();
        let kafka = self.build_kafka(&name, reg);
        let configs = self.build_configs(&name)?;

        let overrides = self.overrides;
        let defaults = overrides.defaults;

        Ok(Manifest {
            name,
            publiclyAccessible: overrides.publicly_accessible.unwrap_or_default(),
            // TODO: Skip most validation if true
            external: simple.external,
            // TODO: Replace with simple.enabled
            disabled: self.disabled,
            // TODO: Must be non-empty
            regions: simple.base.regions,
            // TODO: Make metadata non-optional
            metadata: Some(simple.base.metadata),
            chart: defaults.chart,
            // TODO: Make imageSize non-optional
            imageSize: overrides.image_size.or(Some(512)),
            image: simple.image,
            version: simple.version,
            command: overrides.command.unwrap_or_default(),
            dataHandling: data_handling,
            language: overrides.language,
            resources: overrides.resources,
            replicaCount: defaults.replica_count,
            env: EnvVars {
                plain: defaults.env.iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                secrets: Default::default(),
            },
            secretFiles: overrides.secret_files,
            configs: configs,
            vault: overrides.vault,
            httpPort: overrides.http_port,
            ports: overrides.ports.unwrap_or_default(),
            externalPort: overrides.external_port,
            health: overrides.health,
            dependencies: overrides.dependencies.unwrap_or_default(),
            workers: overrides.workers.unwrap_or_default(),
            sidecars: overrides.sidecars.unwrap_or_default(),
            readinessProbe: overrides.readiness_probe,
            livenessProbe: overrides.liveness_probe,
            lifecycle: overrides.lifecycle,
            rollingUpdate: overrides.rolling_update,
            autoScaling: overrides.auto_scaling,
            tolerations: overrides.tolerations.unwrap_or_default(),
            hostAliases: overrides.host_aliases.unwrap_or_default(),
            initContainers: overrides.init_containers.unwrap_or_default(),
            volumes: overrides.volumes.unwrap_or_default(),
            volumeMounts: overrides.volume_mounts.unwrap_or_default(),
            persistentVolumes: overrides.persistent_volumes.unwrap_or_default(),
            cronJobs: overrides.cron_jobs.unwrap_or_default(),
            jobs: overrides.jobs.unwrap_or_default(),
            serviceAnnotations: overrides.service_annotations,
            labels: overrides.labels.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            kong: simple.kong,
            gate: overrides.gate,
            hosts: overrides.hosts.unwrap_or_default(),
            kafka: kafka,
            sourceRanges: overrides.source_ranges.unwrap_or_default(),
            rbac: overrides.rbac.unwrap_or_default(),

            region: reg.name.clone(),
            environment: reg.environment.to_string(),
            namespace: namespace,
            secrets: Default::default(),
            kind: Default::default(),
        })
    }

    /// Coalesce the namespace from team and region definitions in Shipcat.conf
    /// assumes the region will always have a namespace to fallback on
    /// 
    /// region namespace set | team exists | team namespace set | output
    /// ---------------------+-------------+--------------------+---------------
    /// yes                  | yes         | yes                | team namespace
    /// yes                  | yes         | no                 | region namespace
    /// yes                  | no          | no                 | region namespace
    fn coalesce_namespace(&self, conf: &Config, reg: &Region, team_name: String) -> String {
        match conf.teams.iter().find(|t| t.name == team_name) {
            Some(t) => match t.namespace.clone() {
                Some(n) => n,
                None => reg.namespace.clone(),
            },
            None => reg.namespace.clone(),
        }
    }

    pub fn build_simple(&self, conf: &Config, region: &Region) -> Result<SimpleManifest> {
        let base = self.build_base(conf)?;

        Ok(SimpleManifest {
            region: region.name.to_string(),

            enabled: !self.disabled && base.regions.contains(&region.name),
            external: self.external,

            // TODO: Make image non-optional
            image: Some(self.build_image(&base.name)?),

            version: self.overrides.version.clone(),
            kong: self.build_kong(&base.name, region),

            base,
        })
    }

    pub fn build_base(&self, conf: &Config) -> Result<BaseManifest> {
        let name = self.build_name()?;
        let metadata = self.build_metadata(&name, conf)?;
        let regions = self.regions.clone();

        Ok(BaseManifest {
            name,
            regions,
            metadata,
        })
    }

    fn build_name(&self) -> Result<String> {
        // TODO: Remove and use folder name
        if let Some(name) = &self.name {
            Ok(name.clone())
        } else {
            bail!("name is required")
        }
    }

    fn build_image(&self, service: &String) -> Result<String> {
        if let Some(image) = &self.overrides.image {
            Ok(image.to_string())
        } else if let Some(prefix) = &self.overrides.defaults.image_prefix {
            Ok(format!("{}/{}", prefix, service))
        } else {
            bail!("Image prefix is not defined")
        }
    }

    // TODO: Extract MetadataSource
    fn build_metadata(&self, name: &String, conf: &Config) -> Result<Metadata> {
        if self.metadata.is_none() {
            bail!("Missing metadata for {}", name);
        }
        let mut md = self.metadata.clone().unwrap();

        let team = if let Some(t) = conf.teams.iter().find(|t| t.name == md.team) {
            t
        } else {
            bail!("The team name must match one of the team names in shipcat.conf");
        };
        if md.support.is_none() {
            md.support = team.support.clone();
        }
        if md.notifications.is_none() {
            md.notifications = team.notifications.clone();
        }
        Ok(md.clone())
    }

    // TODO: Extract DataHandlingSource
    fn build_data_handling(&self) -> Option<DataHandling> {
        let original = &self.overrides.data_handling;
        original.clone().map(|mut dh| {
            dh.implicits();
            dh
        })
    }

    // TODO: Extract KafkaSource
    fn build_kafka(&self, service: &String, reg: &Region) -> Option<Kafka> {
        let original = &self.overrides.kafka;
        original.clone().map(|mut kf| {
            kf.implicits(service, reg.clone());
            kf 
        })
    }

    // TODO: Extract Kong
    fn build_kong(&self, service: &String, reg: &Region) -> Option<Kong> {
        let original = &self.overrides.kong;
        original.clone().map(|mut kong| {
            let hosts = self.overrides.hosts.clone().unwrap_or_default();
            kong.implicits(service.clone(), reg.clone(), hosts);
            kong
        })
    }

    // TODO: Extract ConfigsSource
    fn build_configs(&self, service: &String) -> Result<Option<ConfigMap>> {
        let original = &self.overrides.configs;
        if original.is_none() {
            return Ok(None);
        }
        let mut configs = original.clone().unwrap();
        for f in &mut configs.files {
            f.value = Some(read_template_file(service, &f.name)?);
        }
        Ok(Some(configs))
    }

    pub(crate) fn merge_overrides(mut self, other: ManifestOverrides) -> Self {
        self.overrides = self.overrides.merge(other);
        self
    }
}

fn read_template_file(svc: &str, tmpl: &str) -> Result<String> {
    use std::fs::File;
    use std::io::prelude::*;
    use std::path::Path;
    // try to read file from ./services/{svc}/{tmpl} into `tpl` sting
    let pth = Path::new(".").join("services").join(svc).join(tmpl);
    let gpth = Path::new(".").join("templates").join(tmpl);
    let found_pth = if pth.exists() {
        debug!("Reading template in {}", pth.display());
        pth
    } else {
        if !gpth.exists() {
            bail!(
                "Template {} does not exist in neither {} nor {}",
                tmpl,
                pth.display(),
                gpth.display()
            );
        }
        debug!("Reading template in {}", gpth.display());
        gpth
    };
    // read the template - should work now
    let mut f = File::open(&found_pth)?;
    let mut data = String::new();
    f.read_to_string(&mut data)?;
    Ok(data)
}

impl ManifestDefaults {
    pub(crate) fn merge_source(self, mut other: ManifestSource) -> ManifestSource {
        other.overrides.defaults = self.merge(other.overrides.defaults);
        other
    }
}

#[cfg(test)]
mod tests {
    use merge::Merge;
    use std::collections::BTreeMap;

    use super::ManifestDefaults;

    #[test]
    fn merge() {
        let a = ManifestDefaults {
            image_prefix: Option::Some("alpha".into()),
            chart: Option::None,
            replica_count: Option::Some(1),
            env: {
                let mut env = BTreeMap::new();
                env.insert("a".into(), "default-a".into());
                env.insert("b".into(), "default-b".into());
                env
            },
        };
        let b = ManifestDefaults {
            image_prefix: Option::Some("beta".into()),
            chart: Option::Some("default".into()),
            replica_count: None,
            env: {
                let mut env = BTreeMap::new();
                env.insert("b".into(), "override-b".into());
                env.insert("c".into(), "override-c".into());
                env
            },
        };
        let merged = a.merge(b);
        assert_eq!(merged.image_prefix, Option::Some("beta".into()));
        assert_eq!(merged.chart, Option::Some("default".into()));
        assert_eq!(merged.replica_count, Option::Some(1));

        let mut expected_env = BTreeMap::new();
        expected_env.insert("a".into(), "default-a".into());
        expected_env.insert("b".into(), "override-b".into());
        expected_env.insert("c".into(), "override-c".into());
        assert_eq!(merged.env, expected_env);
    }
}
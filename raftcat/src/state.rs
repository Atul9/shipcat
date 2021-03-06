use failure::err_msg;
use tera::compile_templates;
use kube::{
    client::APIClient,
    config::Configuration,
    api::{Reflector, ResourceMap, ApiResource},
};

use std::{
    collections::BTreeMap,
    env,
    sync::{Arc, RwLock},
    time::Duration,
};

use crate::*;
use crate::integrations::{
    newrelic::{self, RelicMap},
    sentryapi::{self, SentryMap},
    version::{self, VersionMap},
};

/// The canonical shared state for actix
///
/// Consumers of these (http handlers) should use public impls on this struct only.
/// Callers should not need to care about getting read/write locks.
/// Only this file should have a write handler to this struct.
#[derive(Clone)]
pub struct State {
    manifests: Reflector<Manifest>,
    configs: Reflector<Config>,
    relics: RelicMap,
    sentries: SentryMap,
    versions: Arc<RwLock<VersionMap>>,
    /// Templates via tera which do not implement clone
    template: Arc<RwLock<tera::Tera>>,
    region: String,
}

/// Note that these functions unwrap a lot and expect errors to just be caught by sentry.
/// The reason we don't return results here is that they are used directly by actix handlers
/// and as such need to be Send:
///
/// Send not implemented for std::sync::PoisonError<std::sync::RwLockReadGuard<'_, T>>
///
/// This is fine; a bad unwrap here or in a handler results in a 500 + a sentry event.
impl State {
    pub fn new(client: APIClient) -> Result<Self> {
        info!("Loading state from CRDs");
        let rname = env::var("REGION_NAME").expect("Need REGION_NAME evar");
        let ns = env::var("NAMESPACE").expect("Need NAMESPACE evar");
        let t = compile_templates!(concat!("raftcat", "/templates/*"));
        debug!("Initializing cache for {} in {}", rname, ns);
        let mfresource = ApiResource {
            group: "babylontech.co.uk".into(),
            resource: "shipcatmanifests".into(),
            namespace: ns.clone(),
        };
        let cfgresource = ApiResource {
            group: "babylontech.co.uk".into(),
            resource: "shipcatconfigs".into(),
            namespace: ns.clone(),
        };
        //let state = DataState::init_cache(client, &ns)?;
        let mut res = State {
            manifests: Reflector::new(client.clone(), mfresource)?,
            configs: Reflector::new(client.clone(), cfgresource)?,
            region: rname,
            relics: BTreeMap::new(),
            sentries: BTreeMap::new(),
            versions: Arc::new(RwLock::new(BTreeMap::new())),
            template: Arc::new(RwLock::new(t)),
        };
        res.update_slow_cache()?;
        Ok(res)
    }
    /// Template getter for main
    pub fn render_template(&self, tpl: &str, ctx: tera::Context) -> String {
        let t = self.template.read().unwrap();
        t.render(tpl, &ctx).unwrap()
    }
    // Getters for main
    pub fn get_manifests(&self) -> Result<ResourceMap<Manifest>> {
        self.manifests.read()
    }
    pub fn get_config(&self) -> Result<Config> {
        let cfgs = self.configs.read()?;
        if let Some(cfg) = cfgs.get(&self.region) {
            Ok(cfg.clone())
        } else {
            bail!("Failed to find config for {}", self.region);
        }
    }
    pub fn get_region(&self) -> Result<Region> {
        let cfg = self.get_config()?;
        match cfg.get_region(&self.region) {
            Ok(r) => Ok(r),
            Err(e) => bail!("could not resolve cluster for {}: {}", self.region, e)
        }
    }
    pub fn get_manifest(&self, key: &str) -> Result<Option<Manifest>> {
        if let Some(mf) = self.manifests.read()?.get(key) {
            return Ok(Some(mf.clone()));
        }
        Ok(None)
    }
    pub fn get_manifests_for(&self, team: &str) -> Result<Vec<String>> {
        let mfs = self.manifests.read()?.iter()
            .filter(|(_k, mf)| mf.metadata.clone().unwrap().team == team)
            .map(|(_k, mf)| mf.name.clone()).collect();
        Ok(mfs)
    }
    pub fn get_reverse_deps(&self, service: &str) -> Result<Vec<String>> {
        let mut res = vec![];
        for (svc, mf) in &self.manifests.read()? {
            if mf.dependencies.iter().any(|d| d.name == service) {
                res.push(svc.clone())
            }
        }
        Ok(res)
    }
    pub fn get_newrelic_link(&self, service: &str) -> Option<String> {
        self.relics.get(service).map(String::to_owned)
    }
    pub fn get_sentry_slug(&self, service: &str) -> Option<String> {
        self.sentries.get(service).map(String::to_owned)
    }
    pub fn get_version(&self, service: &str) -> Option<String> {
        self.versions.read().unwrap().get(service).map(String::to_owned)
    }

    // Interface for internal thread
    fn poll(&self) -> Result<()> {
        self.manifests.poll()?;
        self.configs.poll()?;
        if let Ok(vurl) = std::env::var("VERSION_URL") {
            *self.versions.write().unwrap() = version::get_all(&vurl)?;
        }
        Ok(())
    }

    fn update_slow_cache(&mut self) -> Result<()> {
        let region = self.get_region()?;
        if let Some(s) = region.sentry {
            match sentryapi::get_slugs(&s.url, &region.environment.to_string()) {
                Ok(res) => {
                    self.sentries = res;
                    info!("Loaded {} sentry slugs", self.sentries.len());
                },
                Err(e) => warn!("Unable to load sentry slugs: {}", err_msg(e)),
            }
        } else {
            warn!("No sentry url configured for this region");
        }
        match newrelic::get_links(&region.name) {
            Ok(res) => {
                self.relics = res;
                info!("Loaded {} newrelic links", self.relics.len());
            },
            Err(e) => warn!("Unable to load newrelic projects. {}", err_msg(e)),
        }
        Ok(())
    }
}

/// Initiailize state machine for an actix app
///
/// Returns a Sync
pub fn init(cfg: Configuration) -> Result<State> {
    let client = APIClient::new(cfg);
    let state = State::new(client)?; // for app to read
    let state_clone = state.clone(); // clone for internal thread
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            // update state here - can cause a few more waits in edge cases
            match state_clone.poll() {
                Ok(_) => trace!("State refreshed"), // normal case
                Err(e) => {
                    // Can't recover: boot as much as kubernetes' backoff allows
                    error!("Failed to refesh cache '{}' - rebooting", e);
                    std::process::exit(1); // boot might fix it if network is failing
                }
            }
        }
    });
    Ok(state)
}

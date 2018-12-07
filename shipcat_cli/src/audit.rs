use std::env;

use url::Url;
use chrono::{Utc, SecondsFormat};

use super::{Result, ResultExt, ErrorKind};
use super::{Webhooks, AuditWebhook};
use helm::direct::{UpgradeData, UpgradeState};

/// Payload that gets sent via audit webhook
#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct AuditEvent {
    /// RFC 3339
    pub timestamp: String,
    pub status: UpgradeState,
    /// Eg a jenkins job id
    #[serde(rename = "context_id")]
    pub contextId: Option<String>,
    /// Eg a jenkins job url
    #[serde(rename = "context_link", with = "url_serde", skip_serializing_if = "Option::is_none")]
    pub contextLink: Option<Url>,

    /// represents a single helm upgrade or a reconciliation
    #[serde(flatten)]
    pub payload: AuditDomainObject,
}

impl AuditEvent {
    /// Timestamped payload skeleton
    pub fn new(us: &UpgradeState) -> Self {
        AuditEvent{
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            status: us.clone(),
            contextId: env::var("SHIPCAT_AUDIT_CONTEXT_ID").ok(),
            contextLink: if let Some(urlstr) = env::var("SHIPCAT_AUDIT_CONTEXT_LINK").ok() {
                url::Url::parse(&urlstr).ok()
            } else { None },
            payload: AuditDomainObject::Empty,
        }
    }
}

#[derive(Serialize, Clone)]
#[cfg_attr(test, derive(Debug))]
#[serde(tag = "type", content = "payload", rename_all="snake_case")]
pub enum AuditDomainObject {
    Deployment {
        id: String,
        region: String,
        /// Eg Git SHA
        #[serde(rename = "manifests_revision")]
        manifestsRevision: String,
        service: String,
        version: String,
    },
    Reconciliation {
        id: String,
        region: String,
        /// Eg Git SHA
        #[serde(rename = "manifests_revision")]
        manifestsRevision: String,
    },
    Empty,
}

impl AuditDomainObject {
    pub fn new_deployment(udopt: Option<UpgradeData>) -> Self {
        let (service, region, version) = if let Some(ud) = udopt {
            (ud.name.clone(), ud.region.clone(), ud.version.clone())
        } else {
            ("unknown".into(), "unknown".into(), "unknown".into())
        };
        let manifestsRevision = env::var("SHIPCAT_AUDIT_REVISION").ok().unwrap_or("undefined".into());

        AuditDomainObject::Deployment{
            id: format!("{}-{}-{}-{}", manifestsRevision, region, service, version),
            manifestsRevision, region, service, version,
        }
    }

    pub fn new_reconciliation(r: &str) -> Self {
        let manifestsRevision = env::var("SHIPCAT_AUDIT_REVISION").ok().unwrap_or("undefined".into());

        let region = r.into();
        AuditDomainObject::Reconciliation{
            id: format!("{}-{}", manifestsRevision, region),
            manifestsRevision, region,
        }
    }
}

pub fn ensure_requirements(wh: Option<Webhooks>) -> Result<()> {
    if let Some(_) = &wh {
        // Assume that webhooks strictly contains audit struct if present
        env::var("SHIPCAT_AUDIT_CONTEXT_ID").map_err(|_| ErrorKind::MissingAuditContextId.to_string())?;
        env::var("SHIPCAT_AUDIT_REVISION").map_err(|_| ErrorKind::MissingAuditRevision.to_string())?;
    }
    Ok(())
}

pub fn audit_deployment(us: &UpgradeState, ud: &UpgradeData, audcfg: &AuditWebhook) -> Result<()> {
    let mut ae = AuditEvent::new(&us);
    ae.payload = AuditDomainObject::new_deployment(Some(ud.clone()));
    audit(&ae, &audcfg)
}

pub fn audit_reconciliation(us: &UpgradeState, region: &str, audcfg: &AuditWebhook) -> Result<()> {
    let mut ae = AuditEvent::new(&us);
    ae.payload = AuditDomainObject::new_reconciliation(region);
    audit(&ae, &audcfg)
}

pub fn audit(ae: &AuditEvent, audcfg: &AuditWebhook) -> Result<()> {
    let endpoint = &audcfg.url;
    debug!("event status: {}, url: {:?}", ae.status, endpoint);

    let mkerr = || ErrorKind::Url(endpoint.clone());
    let client = reqwest::Client::new();

    client.post(endpoint.clone())
        .bearer_auth(audcfg.token.clone())
        .json(&ae)
        .send()
        .chain_err(&mkerr)?;
    Ok(())
}

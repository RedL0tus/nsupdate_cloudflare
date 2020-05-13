use anyhow::{bail, Error};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::parser::NSUpdateAction;
use super::parser::NSUpdateActionAdd;
use super::parser::NSUpdateActionDelete;
use super::parser::NSUpdateCommand;
use super::parser::NSUpdateQueue;

#[derive(Debug, Serialize)]
pub struct RequestDataAdd {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    ttl: usize,
    priority: Option<usize>,
    proxied: bool,
}

#[derive(Clone, Debug)]
pub struct RequestDataDelete {
    pub record_type: String,
    pub name: String,
}

#[derive(Debug)]
pub struct RequestQueue {
    inner: Vec<RequestData>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CFRecord {
    id: String,
    #[serde(default)]
    #[serde(rename = "type")]
    record_type: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    ttl: usize,
    #[serde(default)]
    locked: bool,
    #[serde(default)]
    zone_id: String,
    #[serde(default)]
    zone_name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct CFError {
    code: usize,
    message: String,
}

#[derive(Debug, Default, Deserialize)]
struct CFResultInfo {
    page: usize,
    per_page: usize,
    count: usize,
    total_count: usize,
    pub total_pages: usize,
}

#[derive(Debug, Default, Deserialize)]
struct CFListResponse {
    success: bool,
    errors: Vec<CFError>,
    #[serde(skip)]
    messages: (),
    result: Vec<CFRecord>,
    result_info: CFResultInfo,
}

#[derive(Debug, Default)]
struct CFCurrentRecords {
    inner: Vec<CFRecord>,
}

#[derive(Clone, Debug, Deserialize)]
struct CFUpdateResponse {
    success: bool,
    errors: Vec<CFError>,
    result: Option<CFRecord>,
}

#[derive(Debug)]
pub enum RequestData {
    Add(RequestDataAdd),
    Delete(RequestDataDelete),
}

impl From<NSUpdateActionAdd> for RequestDataAdd {
    fn from(source: NSUpdateActionAdd) -> Self {
        Self {
            record_type: source.record_type,
            name: source.domain,
            content: source.content,
            ttl: source.ttl,
            priority: source.priority,
            proxied: false,
        }
    }
}

impl From<NSUpdateActionDelete> for RequestDataDelete {
    fn from(source: NSUpdateActionDelete) -> Self {
        Self {
            record_type: source.record_type,
            name: source.domain,
        }
    }
}

impl From<NSUpdateQueue> for RequestQueue {
    fn from(source: NSUpdateQueue) -> Self {
        Self {
            inner: source
                .into_inner()
                .into_iter()
                .filter_map(|command| {
                    if let NSUpdateCommand::Update(update) = command {
                        Some(match update {
                            NSUpdateAction::Add(orig_add) => {
                                RequestData::Add(RequestDataAdd::from(orig_add))
                            }
                            NSUpdateAction::Delete(orig_delete) => {
                                RequestData::Delete(RequestDataDelete::from(orig_delete))
                            }
                        })
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }
}

impl CFListResponse {
    async fn new(zone_id: &str, token: &str, page: usize) -> Result<Self, Error> {
        match surf::get(format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?per_page=1000&page={}",
            zone_id, page
        ))
        .set_header(
            "Authorization".parse().expect("Wut?"),
            format!("Bearer {}", token),
        )
        .recv_json()
        .await
        {
            // Surf sucks at this part
            Ok(response) => Ok(response),
            Err(err) => bail!(err),
        }
    }

    async fn get_records(self) -> Result<Vec<CFRecord>, Error> {
        Ok(self.result)
    }

    async fn get_total_pages(&self) -> Result<usize, Error> {
        if !self.success {
            bail!(format!(
                "Failed to request records from CloudFlare: {:?}",
                &self.errors
            ));
        }
        Ok(self.result_info.total_pages)
    }
}

impl CFCurrentRecords {
    async fn new() -> Self {
        Self {
            ..Default::default()
        }
    }

    async fn append(&mut self, result: &mut Vec<CFRecord>) {
        debug!(">>> Appending : {:?}", &result);
        self.inner.append(result);
    }

    async fn update(&mut self, zone_id: &str, token: &str) -> Result<(), Error> {
        info!("Updating record list");
        let first_record = CFListResponse::new(zone_id, token, 1).await?;
        let total_pages = first_record.get_total_pages().await?;
        self.append(&mut first_record.get_records().await?).await;
        if total_pages > 1 {
            for page in 2usize..=total_pages {
                self.append(
                    &mut CFListResponse::new(zone_id, token, page)
                        .await?
                        .get_records()
                        .await?,
                )
                .await;
            }
        }
        info!("Received {} records", self.inner.len());
        debug!("Current records: {:?}", &self);
        Ok(())
    }

    async fn find_record(&self, name: &str, record_type: &str) -> Result<Option<&CFRecord>, Error> {
        if self.inner.is_empty() {
            Ok(None)
        } else {
            debug!("Finding record ID");
            Ok(self.inner.iter().find(|&record| {
                debug!(
                    ">>> Target: {:?}, current: {:?}",
                    format!("{}.", &record.name),
                    name
                );
                (format!("{}.", record.name) == name) && (record.record_type == record_type)
            }))
        }
    }

    async fn find_record_id(&self, name: &str, record_type: &str) -> Result<Option<&str>, Error> {
        if let Some(record) = self.find_record(name, record_type).await? {
            Ok(Some(&record.id))
        } else {
            Ok(None)
        }
    }
}

impl RequestDataAdd {
    async fn send(self, zone_id: &str, token: &str) -> Result<Option<CFUpdateResponse>, Error> {
        info!("Adding {}", self.name);
        match surf::post(format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
            zone_id
        ))
        .set_header(
            "Authorization".parse().expect("Wut?"),
            format!("Bearer {}", token),
        )
        .body_json(&json!(self))?
        .recv_json()
        .await
        {
            Ok(response) => Ok(Some(response)),
            Err(err) => bail!(err),
        }
    }
}

impl RequestDataDelete {
    async fn send(
        self,
        zone_id: &str,
        token: &str,
        record_id: Option<&str>,
    ) -> Result<Option<CFUpdateResponse>, Error> {
        info!("Deleting {}", self.name);
        if record_id.is_none() {
            warn!("Record not found");
            return Ok(None);
        }
        match surf::delete(format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            zone_id,
            record_id.expect("Wut?")
        ))
        .set_header(
            "Authorization".parse().expect("Wut?"),
            format!("Bearer {}", token),
        )
        .recv_json()
        .await
        {
            Ok(response) => Ok(Some(response)),
            Err(err) => bail!(err),
        }
    }
}

impl RequestData {
    async fn send(
        self,
        zone_id: &str,
        token: &str,
        current_records: &CFCurrentRecords,
    ) -> Result<Option<CFUpdateResponse>, Error> {
        Ok(match self {
            Self::Add(request_add) => request_add.send(zone_id, token).await?,
            Self::Delete(request_delete) => {
                let domain = request_delete.clone().name;
                let record_type = request_delete.clone().record_type;
                let record_id = current_records
                    .find_record_id(&domain, &record_type)
                    .await?;
                debug!("Record ID: {:?}, domain: {:?}", &record_id, &domain);
                request_delete.send(zone_id, token, record_id).await?
            }
        })
    }
}

impl RequestQueue {
    pub async fn process(self, zone_id: &str, token: &str) -> Result<(usize, usize), Error> {
        let mut current_records = CFCurrentRecords::new().await;
        current_records.update(zone_id, token).await?;
        let iterator = self.inner.into_iter();
        let mut subtotal: usize = 0;
        let mut subtotal_failed: usize = 0;
        for request in iterator {
            let result = request.send(zone_id, token, &current_records).await?;
            subtotal += 1;
            info!("Result: {}", {
                if result.is_some() && result.clone().expect("Wut?").success {
                    "SUCCESS"
                } else {
                    subtotal_failed += 1;
                    "FAILED"
                }
            });
            debug!("Result: {:?}", result);
        }
        Ok((subtotal, subtotal_failed))
    }
}

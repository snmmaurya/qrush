use serde::{Serialize, de::DeserializeOwned};
use crate::job::Job;
use crate::registry::JOB_REGISTRY;


pub trait JobMetadata: Serialize + DeserializeOwned {
    fn queue() -> &'static str;
    fn name() -> &'static str;
    fn max_retries() -> usize {
        0
    }
}




pub async fn parse_dynamic_job(json_str: &str) -> Option<Box<dyn Job>> {
    let val: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let job_type = val.get("type")?.as_str()?;

    let registry = JOB_REGISTRY.lock().unwrap();
    let handler = registry.get(job_type)?;

    handler(json_str.to_string()).await.ok()
}

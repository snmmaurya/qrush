// src/utils/jconfig.rs

use crate::registry::get_job_handler;
use crate::job::Job;
use anyhow::Result;
use serde::Serialize;
use crate::utils::rdconfig::get_redis_connection;
use redis::AsyncCommands;

#[derive(Serialize, Debug, Clone)]
pub struct JobInfo {
    pub id: String,
    pub job_type: String,
    pub queue: Option<String>,
    pub payload: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub run_at: Option<String>,
    pub error: Option<String>,
    pub failed_at: Option<String>,
}

pub fn to_job_info(job: &Box<dyn Job>, id: &str) -> JobInfo {
    JobInfo {
        id: id.to_string(),
        job_type: job.name().to_string(),
        queue: Some(job.queue().to_string()),
        payload: Some("N/A".to_string()),
        status: None,
        created_at: None,
        updated_at: None,
        failed_at: None,
        error: None,
        run_at: None,
    }
}

pub fn extract_job_type(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    v.get("job_type")?.as_str().map(String::from)
}

pub async fn deserialize_job(payload: String) -> Option<Box<dyn Job>> {
    let job_type = extract_job_type(&payload)?;
    let handler = get_job_handler(&job_type)?;
    match handler(payload).await {
        Ok(job) => Some(job),
        Err(err) => {
            tracing::error!("Failed to deserialize job '{}': {:?}", job_type, err);
            None
        }
    }
}

pub async fn fetch_job_info(job_id: &str) -> Result<Option<JobInfo>> {
    let job_key = format!("snm:job:{job_id}");
    let mut conn = get_redis_connection().await?;

    let map: redis::RedisResult<redis::Value> = conn.hgetall(&job_key).await;

    if let Ok(redis::Value::Array(items)) = map {  // Changed from Bulk to Array
        if items.is_empty() {
            return Ok(None);
        }

        let mut job_info = JobInfo {
            id: job_id.to_string(),
            job_type: "unknown".to_string(),
            queue: None,
            status: None,
            payload: None,
            created_at: None,
            updated_at: None,
            run_at: None,
            error: None,
            failed_at: None,
        };

        for chunk in items.chunks(2) {
            if let [redis::Value::BulkString(field), redis::Value::BulkString(value)] = chunk {  // Changed from Data to BulkString
                let key = String::from_utf8_lossy(field);
                let val = String::from_utf8_lossy(value);

                match key.as_ref() {
                    "queue" => job_info.queue = Some(val.to_string()),
                    "status" => job_info.status = Some(val.to_string()),
                    "payload" => {
                        job_info.payload = Some(val.to_string());
                        if let Some(job_type) = extract_job_type(&val) {
                            job_info.job_type = job_type;
                        }
                    }
                    "created_at" => job_info.created_at = Some(val.to_string()),
                    "updated_at" => job_info.updated_at = Some(val.to_string()),
                    "run_at" => job_info.run_at = Some(val.to_string()),
                    "error" => job_info.error = Some(val.to_string()),
                    "failed_at" => job_info.failed_at = Some(val.to_string()),
                    _ => {}
                }
            }
        }

        return Ok(Some(job_info));
    }

    Ok(None)
}
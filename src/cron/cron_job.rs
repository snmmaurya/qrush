// src/cron/cron_job.rs
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::job::Job;

/// Trait for jobs that can be scheduled with cron expressions
#[async_trait]
pub trait CronJob: Job + Send + Sync {
    /// Cron expression (e.g., "0 */5 * * * *" for every 5 minutes)
    fn cron_expression(&self) -> &'static str;
    
    /// Unique identifier for this cron job
    fn cron_id(&self) -> &'static str;
    
    /// Whether this cron job is enabled
    fn enabled(&self) -> bool {
        true
    }
    
    /// Timezone for cron execution (default UTC)
    fn timezone(&self) -> &'static str {
        "UTC"
    }
}

/// Metadata for storing cron jobs in Redis
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CronJobMeta {
    pub id: String,
    pub name: String,
    pub queue: String,
    pub cron_expression: String,
    pub timezone: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: String,
    pub created_at: String,
    pub payload: String, // Serialized job data
}

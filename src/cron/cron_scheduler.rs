// src/cron/cron_scheduler.rs
use std::collections::HashMap;
use tokio::time::{sleep, Duration};
use redis::AsyncCommands;
use chrono::Utc;
use anyhow::Result;
use tracing::{info, error, warn};
use futures::FutureExt; // Add this import

use crate::config::get_shutdown_notify;
use crate::utils::rdconfig::get_redis_connection;
use crate::cron::cron_parser::CronParser;
use crate::cron::cron_job::CronJobMeta;
// Import the CronJob trait
use crate::cron::cron_job::CronJob;

const CRON_JOBS_KEY: &str = "snm:cron:jobs";
const CRON_SCHEDULE_KEY: &str = "snm:cron:schedule";

pub struct CronScheduler;

impl CronScheduler {
    /// Start the cron scheduler worker
    pub async fn start() {
        let shutdown = get_shutdown_notify();
        
        tokio::spawn(async move {
            info!("🕐 Cron scheduler started");
            
            loop {
                if shutdown.notified().now_or_never().is_some() {
                    info!("🕐 Cron scheduler shutting down");
                    break;
                }

                if let Err(e) = Self::process_cron_jobs().await {
                    error!("Cron scheduler error: {:?}", e);
                }

                // Check every 30 seconds
                sleep(Duration::from_secs(30)).await;
            }
        });
    }

    /// Register a cron job - we'll store the serialized payload directly
    pub async fn register_cron_job<T>(job: T) -> Result<()> 
    where
        T: CronJob + serde::Serialize + 'static,
    {
        let mut conn = get_redis_connection().await?;
        let now = Utc::now();
        
        let next_run = CronParser::next_execution(job.cron_expression(), now)?;
        
        let meta = CronJobMeta {
            id: job.cron_id().to_string(),
            name: job.name().to_string(),
            queue: job.queue().to_string(),
            cron_expression: job.cron_expression().to_string(),
            timezone: job.timezone().to_string(),
            enabled: job.enabled(),
            last_run: None,
            next_run: next_run.to_rfc3339(),
            created_at: now.to_rfc3339(),
            payload: serde_json::to_string(&job)?,
        };

        // Store cron job metadata
        let job_key = format!("{}:{}", CRON_JOBS_KEY, meta.id);
        conn.hset_multiple::<_, _, _, ()>(&job_key, &[
            ("name", &meta.name),
            ("queue", &meta.queue),
            ("cron_expression", &meta.cron_expression),
            ("timezone", &meta.timezone),
            ("enabled", &meta.enabled.to_string()),
            ("next_run", &meta.next_run),
            ("created_at", &meta.created_at),
            ("payload", &meta.payload),
        ]).await?;

        // Add to schedule (sorted set by next_run timestamp)
        conn.zadd::<_, _, _, ()>(
            CRON_SCHEDULE_KEY,
            &meta.id,
            next_run.timestamp()
        ).await?;

        info!("📅 Registered cron job: {} ({})", meta.name, meta.cron_expression);
        Ok(())
    }

    /// Process due cron jobs
    async fn process_cron_jobs() -> Result<()> {
        let mut conn = get_redis_connection().await?;
        let now = Utc::now();
        let now_timestamp = now.timestamp();

        // Get all jobs that are due to run
        let due_jobs: Vec<String> = conn
            .zrangebyscore(CRON_SCHEDULE_KEY, 0, now_timestamp)
            .await?;

        for job_id in due_jobs {
            if let Err(e) = Self::execute_cron_job(&job_id).await {
                error!("Failed to execute cron job {}: {:?}", job_id, e);
            }
        }

        Ok(())
    }

    /// Execute a single cron job by directly enqueueing the payload
    async fn execute_cron_job(job_id: &str) -> Result<()> {
        let mut conn = get_redis_connection().await?;
        let job_key = format!("{}:{}", CRON_JOBS_KEY, job_id);
        
        // Get job metadata
        let job_data: HashMap<String, String> = conn.hgetall(&job_key).await?;
        
        if job_data.is_empty() {
            warn!("Cron job {} not found", job_id);
            return Ok(());
        }

        let enabled: bool = job_data
            .get("enabled")
            .and_then(|v| v.parse().ok())
            .unwrap_or(false);

        if !enabled {
            info!("Skipping disabled cron job: {}", job_id);
            return Ok(());
        }

        let cron_expr = job_data.get("cron_expression")
            .ok_or_else(|| anyhow::anyhow!("Missing cron_expression"))?;
        
        let payload = job_data.get("payload")
            .ok_or_else(|| anyhow::anyhow!("Missing payload"))?;

        // FIX: Create a proper owned String instead of borrowing from temporary
        let queue = job_data.get("queue")
            .map(|s| s.clone())
            .unwrap_or_else(|| "default".to_string());

        // Instead of deserializing and re-enqueueing, directly enqueue the raw job
        let enqueued_job_id = Self::enqueue_raw_job(payload, &queue).await?;
        info!("🚀 Cron job {} enqueued as {}", job_id, enqueued_job_id);

        // Calculate next run time
        let now = Utc::now();
        match CronParser::next_execution(cron_expr, now) {
            Ok(next_run) => {
                // Update last_run and next_run
                conn.hset_multiple::<_, _, _, ()>(&job_key, &[
                    ("last_run", &now.to_rfc3339()),
                    ("next_run", &next_run.to_rfc3339()),
                ]).await?;

                // Update schedule
                conn.zadd::<_, _, _, ()>(
                    CRON_SCHEDULE_KEY,
                    job_id,
                    next_run.timestamp()
                ).await?;

                info!("📅 Cron job {} rescheduled for {}", job_id, next_run);
            }
            Err(e) => {
                error!("Failed to calculate next run for cron job {}: {:?}", job_id, e);
                // Remove from schedule if we can't calculate next run
                conn.zrem::<_, _, ()>(CRON_SCHEDULE_KEY, job_id).await?;
            }
        }

        Ok(())
    }

    /// Enqueue a raw job payload directly (bypasses the type system)
    async fn enqueue_raw_job(payload: &str, queue: &str) -> Result<String> {
        use crate::utils::constants::{PREFIX_JOB, PREFIX_QUEUE};
        use nanoid::nanoid;

        let mut conn = get_redis_connection().await?;
        let job_id = nanoid!(10);
        let now = Utc::now().to_rfc3339();

        let queue_key = format!("{PREFIX_QUEUE}:{}", queue);
        let job_key = format!("{PREFIX_JOB}:{job_id}");

        // Store the job with the exact same structure as regular jobs
        conn.hset_multiple::<_, _, _, ()>(&job_key, &[
            ("queue", queue),
            ("status", "pending"),
            ("payload", payload),
            ("created_at", &now),
        ]).await?;

        // Add to queue
        conn.rpush::<_, _, ()>(&queue_key, &job_id).await?;
        conn.sadd::<_, _, ()>("snm:queues", queue).await?;

        Ok(job_id)
    }

    /// List all registered cron jobs
    pub async fn list_cron_jobs() -> Result<Vec<CronJobMeta>> {
        let mut conn = get_redis_connection().await?;
        let keys: Vec<String> = conn.keys(format!("{}:*", CRON_JOBS_KEY)).await?;
        
        let mut jobs = Vec::new();
        
        for key in keys {
            let job_data: HashMap<String, String> = conn.hgetall(&key).await?;
            
            if let Some(id) = key.strip_prefix(&format!("{}:", CRON_JOBS_KEY)) {
                let job_meta = CronJobMeta {
                    id: id.to_string(),
                    name: job_data.get("name").unwrap_or(&"Unknown".to_string()).clone(),
                    queue: job_data.get("queue").unwrap_or(&"default".to_string()).clone(),
                    cron_expression: job_data.get("cron_expression").unwrap_or(&"".to_string()).clone(),
                    timezone: job_data.get("timezone").unwrap_or(&"UTC".to_string()).clone(),
                    enabled: job_data.get("enabled").and_then(|v| v.parse().ok()).unwrap_or(false),
                    last_run: job_data.get("last_run").cloned(),
                    next_run: job_data.get("next_run").unwrap_or(&"".to_string()).clone(),
                    created_at: job_data.get("created_at").unwrap_or(&"".to_string()).clone(),
                    payload: job_data.get("payload").unwrap_or(&"".to_string()).clone(),
                };
                jobs.push(job_meta);
            }
        }
        
        Ok(jobs)
    }

    /// Enable/disable a cron job
    pub async fn toggle_cron_job(job_id: &str, enabled: bool) -> Result<()> {
        let mut conn = get_redis_connection().await?;
        let job_key = format!("{}:{}", CRON_JOBS_KEY, job_id);
        
        conn.hset::<_, _, _, ()>(&job_key, "enabled", enabled.to_string()).await?;
        
        if enabled {
            info!("✅ Enabled cron job: {}", job_id);
            // When re-enabling, recalculate next run time
            let job_data: HashMap<String, String> = conn.hgetall(&job_key).await?;
            if let Some(cron_expr) = job_data.get("cron_expression") {
                let now = Utc::now();
                if let Ok(next_run) = CronParser::next_execution(cron_expr, now) {
                    conn.hset::<_, _, _, ()>(&job_key, "next_run", next_run.to_rfc3339()).await?;
                    conn.zadd::<_, _, _, ()>(CRON_SCHEDULE_KEY, job_id, next_run.timestamp()).await?;
                }
            }
        } else {
            info!("❌ Disabled cron job: {}", job_id);
            // Remove from schedule when disabled
            conn.zrem::<_, _, ()>(CRON_SCHEDULE_KEY, job_id).await?;
        }
        
        Ok(())
    }

    /// Delete a cron job
    pub async fn delete_cron_job(job_id: &str) -> Result<()> {
        let mut conn = get_redis_connection().await?;
        let job_key = format!("{}:{}", CRON_JOBS_KEY, job_id);
        
        conn.del::<_, ()>(&job_key).await?;
        conn.zrem::<_, _, ()>(CRON_SCHEDULE_KEY, job_id).await?;
        
        info!("🗑️ Deleted cron job: {}", job_id);
        Ok(())
    }

    /// Run a cron job immediately (outside of schedule)
    pub async fn run_now(job_id: &str) -> Result<String> {
        let mut conn = get_redis_connection().await?;
        let job_key = format!("{}:{}", CRON_JOBS_KEY, job_id);
        
        let job_data: HashMap<String, String> = conn.hgetall(&job_key).await?;
        
        if job_data.is_empty() {
            return Err(anyhow::anyhow!("Cron job {} not found", job_id));
        }

        let payload = job_data.get("payload")
            .ok_or_else(|| anyhow::anyhow!("Missing payload"))?;
        
        // FIX: Create a proper owned String instead of borrowing from temporary
        let queue = job_data.get("queue")
            .map(|s| s.clone())
            .unwrap_or_else(|| "default".to_string());

        let enqueued_job_id = Self::enqueue_raw_job(payload, &queue).await?;
        info!("🚀 Manually triggered cron job {} as {}", job_id, enqueued_job_id);
        
        Ok(enqueued_job_id)
    }
}
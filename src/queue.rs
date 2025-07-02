// src/queue.rs
use crate::job::Job;
use crate::redis_pool::get_redis_conn;
use serde::Serialize;
use serde_json::to_string;
use redis::AsyncCommands;
use chrono::Utc;
use nanoid::nanoid;

const PREFIX_QUEUE: &str = "snm:queue";
const DELAYED_JOBS_KEY: &str = "snm:delayed_jobs";

/// Push a job immediately into the Redis queue and store metadata
pub async fn enqueue<J: Job + Serialize>(job: J) -> anyhow::Result<()> {
    let mut conn = get_redis_conn().await?;
    let payload = to_string(&job)?;
    let job_id = nanoid!(10);
    let now = Utc::now().to_rfc3339();

    let queue_key = format!("{PREFIX_QUEUE}:{}", J::queue());
    let job_key = format!("snm:job:{job_id}");

    // Store job metadata (job_id → job info)
    conn.hset_multiple(&job_key, &[
        ("queue", J::queue()),
        ("status", "pending"),
        ("payload", &payload),
        ("created_at", &now),
    ]).await?;

    // Push job ID to queue (not raw payload)
    conn.rpush::<_, _, ()>(&queue_key, &job_id).await?;

    println!("✅ Enqueued job ID: {job_id}");
    Ok(())
}

/// Schedule a job for delayed execution
pub async fn enqueue_in<J: Job + Serialize>(job: J, delay_secs: u64) -> anyhow::Result<()> {
    let mut conn = get_redis_conn().await?;
    let payload = to_string(&job)?;
    let job_id = nanoid!(10);
    let now = Utc::now().to_rfc3339();
    let run_at = Utc::now().timestamp() + delay_secs as i64;

    let job_key = format!("snm:job:{job_id}");

    // Store job metadata
    conn.hset_multiple(&job_key, &[
        ("queue", J::queue()),
        ("status", "delayed"),
        ("payload", &payload),
        ("created_at", &now),
        ("run_at", &run_at.to_string()),
    ]).await?;

    // Add job ID to delayed set
    conn.zadd(DELAYED_JOBS_KEY, &job_id, run_at).await?;

    println!("⏳ Delayed job ID: {job_id} (run at {run_at})");
    Ok(())
}

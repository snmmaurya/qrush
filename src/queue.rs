// src/queue.rs
use crate::job::Job;
use crate::utils::rdconfig::get_redis_connection;
use crate::utils::constants::{
    PREFIX_JOB,
    DELAYED_JOBS_KEY,
    PREFIX_QUEUE,
};


use serde::Serialize;
use serde_json::to_string;
use redis::AsyncCommands;
use chrono::Utc;
use nanoid::nanoid;

pub async fn enqueue<J>(job: J) -> anyhow::Result<String>
where
    J: Job + Serialize,
{
    let mut conn = get_redis_connection().await?;
    let payload = to_string(&job)?;
    let job_id = nanoid!(10);
    let now = Utc::now().to_rfc3339();

    let queue = job.queue();
    let queue_key = format!("{PREFIX_QUEUE}:{}", queue);
    let job_key = format!("{PREFIX_JOB}:{job_id}");

    conn.hset_multiple::<_, _, _, ()>(&job_key, &[
        ("queue", queue),
        ("status", "pending"),
        ("payload", &payload),
        ("created_at", &now),
    ]).await?;

    conn.rpush::<_, _, ()>(&queue_key, &job_id).await?;
    conn.sadd::<_, _, ()>("snm:queues", queue).await?;

    println!("✅ Enqueued job: {} in queue: {}", job_id, queue);
    Ok(job_id)
}





pub async fn enqueue_in<J>(job: J, delay_secs: u64) -> anyhow::Result<String>
where
    J: Job + Serialize,
{
    let mut conn = get_redis_connection().await?;
    let payload = to_string(&job)?;
    let job_id = nanoid!(10);
    let now = Utc::now().to_rfc3339();
    let run_at = Utc::now().timestamp() + delay_secs as i64;

    let queue = job.queue();
    let job_key = format!("{PREFIX_JOB}:{job_id}");

    conn.hset_multiple::<_, _, _, ()>(&job_key, &[
        ("queue", queue),
        ("status", "delayed"),
        ("payload", &payload),
        ("created_at", &now),
        ("run_at", &run_at.to_string()),
    ]).await?;

    conn.zadd::<_, _, _, ()>(DELAYED_JOBS_KEY, &job_id, run_at).await?;
    conn.sadd::<_, _, ()>("snm:queues", queue).await?;

    println!("⏳ Scheduled job: {} to run at: {}", job_id, run_at);
    Ok(job_id)
}

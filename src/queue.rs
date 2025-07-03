use crate::job::Job;
use crate::rdconfig::get_redis_conn;
use crate::constants::{DELAYED_JOBS_KEY, PREFIX_QUEUE};

use serde::Serialize;
use serde_json::to_string;
use redis::AsyncCommands;
use chrono::Utc;
use nanoid::nanoid;

/// Enqueues a job immediately into its specified queue.
pub async fn enqueue<J>(job: J) -> anyhow::Result<()>
where
    J: Job + Serialize +,
{
    let mut conn = get_redis_conn().await?;
    let payload = to_string(&job)?;
    let job_id = nanoid!(10);
    let now = Utc::now().to_rfc3339();

    let queue_key = format!("{PREFIX_QUEUE}:{}", <J as Job>::queue());
    let job_key = format!("snm:job:{job_id}");

    conn.hset_multiple::<_, _, _, ()>(&job_key, &[
        ("queue", <J as Job>::queue()),
        ("status", "pending"),
        ("payload", &payload),
        ("created_at", &now),
    ]).await?;

    conn.rpush::<_, _, ()>(&queue_key, &job_id).await?;

    println!("✅ Enqueued job ID: {job_id}");
    Ok(())
}

/// Enqueues a job with a delay in seconds.
pub async fn enqueue_in<J>(job: J, delay_secs: u64) -> anyhow::Result<()>
where
    J: Job + Serialize +,
{
    let mut conn = get_redis_conn().await?;
    let payload = to_string(&job)?;
    let job_id = nanoid!(10);
    let now = Utc::now().to_rfc3339();
    let run_at = Utc::now().timestamp() + delay_secs as i64;

    let job_key = format!("snm:job:{job_id}");

    conn.hset_multiple::<_, _, _, ()>(&job_key, &[
        ("queue", <J as Job>::queue()),
        ("status", "delayed"),
        ("payload", &payload),
        ("created_at", &now),
        ("run_at", &run_at.to_string()),
    ]).await?;

    conn.zadd::<_, _, _, ()>(DELAYED_JOBS_KEY, &job_id, run_at).await?;

    println!("⏳ Delayed job ID: {job_id} (run at {run_at})");
    Ok(())
}

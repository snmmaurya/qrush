use crate::job::Job;
use crate::redis_pool::get_redis_conn;
use serde_json::to_string;
use redis::AsyncCommands;
use serde::Serialize;

/// Push a serialized job payload to a Redis queue
pub async fn enqueue<J: Job>(job: J) -> anyhow::Result<()> {
    let mut conn = get_redis_conn().await?;
    let payload = to_string(&job)?;
    let key = format!("snm:queue:{}", J::queue()); // <-- fixed here

    conn.rpush::<_, _, ()>(key, payload).await?;
    Ok(())
}

/// Schedule a job to be enqueued after a delay
pub async fn enqueue_in<J: Job + Send + Serialize + 'static>(job: J, delay_secs: u64) -> anyhow::Result<()> {
    let redis = get_redis_conn().await?;
    let payload = serde_json::to_string(&job)?;
    let timestamp = chrono::Utc::now().timestamp() + delay_secs as i64;

    let mut conn = redis;
    conn.zadd("snm:delayed_jobs", payload, timestamp).await?;

    Ok(())
}
use crate::{registry::get_registered_jobs, redis_pool::get_redis_conn};
use crate::config::QueueConfig;
use tokio::time::{sleep, Duration};
use redis::AsyncCommands;
use tracing::{info, error, debug};


pub async fn start_worker_pool(queue: &str, concurrency: usize) {
    for i in 0..concurrency {
        let queue = queue.to_string();
        tokio::spawn(async move {
            loop {
                let mut conn = match get_redis_conn().await {
                    Ok(c) => c,
                    Err(_) => {
                        sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };

                let payload: Option<String> = conn.lpop(format!("snm:queue:{}", queue), None).await.unwrap_or(None);
                if let Some(job_json) = payload.clone() {

                    println!("payload: {:?}", payload);

                    let jobs = get_registered_jobs();
                    for (name, handler) in jobs {
                        let fut = handler(job_json.clone());
                        if fut.await.is_ok() {
                            break;
                        }
                    }
                }

                sleep(Duration::from_millis(500)).await;
            }
        });
    }
}




pub async fn start_delayed_worker_pool() {
    tokio::spawn(async move {
        loop {
            let now = chrono::Utc::now().timestamp();
            let mut conn = match get_redis_conn().await {
                Ok(c) => c,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            let jobs: Vec<String> = conn.zrangebyscore("snm:delayed_jobs", 0, now).await.unwrap_or_default();
            for job_str in jobs {
                // Push to main queue
                let _: () = conn.lpush("snm:queue:default", &job_str).await.unwrap_or_default();
                let _: () = conn.zrem("snm:delayed_jobs", &job_str).await.unwrap_or_default();
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

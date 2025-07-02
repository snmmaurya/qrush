// src/runner.rs

use crate::{registry::get_registered_jobs, redis_pool::get_redis_conn};
use tokio::time::{sleep, Duration};
use redis::AsyncCommands;
use chrono::Utc;



pub async fn start_worker_pool(queue: &str, concurrency: usize) {
    for _i in 0..concurrency {
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

                let job_id: Option<String> = conn
                    .lpop(format!("snm:queue:{}", queue), None)
                    .await
                    .unwrap_or(None);

                if let Some(job_id) = job_id {
                    let job_key = format!("snm:job:{job_id}");
                    let job_payload: String = conn.hget(&job_key, "payload").await.unwrap_or_default();

                    let jobs = get_registered_jobs();
                    let mut handled = false;

                    for (_, handler) in jobs {
                        match handler(job_payload.clone()).await {
                            Ok(_) => {
                                handled = true;

                                // ✅ SUCCESS: Update job and Redis metrics
                                let _: () = conn.hset_multiple(&job_key, &[
                                    ("status", "success"),
                                    ("completed_at", &Utc::now().to_rfc3339()),
                                ]).await.unwrap_or_default();

                                let _: () = conn.incr("snm:qrush:success", 1).await.unwrap_or_default();

                                let _: Result<(), _> = conn
                                .lpush(
                                    format!("snm:logs:{}", queue),
                                    format!("[{}] ✅ Job {} succeeded", Utc::now(), job_id),
                                )
                                .await;

                                let _: Result<(), _> = conn
                                .ltrim(format!("snm:logs:{}", queue), 0, 99)
                                .await;


                                break;
                            }
                            Err(_) => {
                                // continue to next handler
                            }
                        }
                    }

                    if !handled {
                        // ❌ FAILED: Update job and metrics
                        let _: () = conn.hset_multiple(&job_key, &[
                            ("status", "failed"),
                            ("completed_at", &Utc::now().to_rfc3339()),
                        ]).await.unwrap_or_default();

                        let _: () = conn.incr("snm:qrush:failed", 1).await.unwrap_or_default();

                        let _: Result<(), _> = conn
                        .lpush(
                            format!("snm:logs:{}", queue),
                            format!("[{}] ❌ Job {} failed", Utc::now(), job_id),
                        )
                        .await;

                        let _: Result<(), _> = conn
                        .ltrim(format!("snm:logs:{}", queue), 0, 99)
                        .await;

                        let job_key = format!("snm:job:{}", job_id);
                        let _: () = conn.hset_multiple(
                            &job_key,
                            &[
                                ("status", "failed"),
                                ("failed_at", Utc::now().to_rfc3339().as_str()),
                                ("queue", &queue),
                            ],
                        )
                        .await
                        .unwrap_or_default();

                        let _: () = conn
                            .rpush("snm:failed_jobs", job_id.clone())
                            .await
                            .unwrap_or_default();
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

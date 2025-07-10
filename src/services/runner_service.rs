
use crate::{registry::get_registered_jobs, config::get_shutdown_notify};
use crate::utils::rdconfig::get_redis_connection;
use tokio::time::{sleep, Duration};
use redis::AsyncCommands;
use chrono::Utc;
use futures::FutureExt; // Required for `.now_or_never()`
use crate::utils::constants::{DELAYED_JOBS_KEY, MAX_RETRIES};



pub async fn start_worker_pool(queue: &str, concurrency: usize) {
    let shutdown = get_shutdown_notify();    
    for _ in 0..concurrency {
        let queue = queue.to_string();
        let shutdown = shutdown.clone();

        tokio::spawn(async move {
            let mut error_message: Option<String> = None;
            let today = Utc::now().date_naive().format("%Y-%m-%d").to_string();

            loop {
                if shutdown.notified().now_or_never().is_some() {
                    break;
                }

                let mut conn = match get_redis_connection().await {
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
                    let job_key = format!("snm:job:{}", job_id);
                    let job_payload: String = conn.hget(&job_key, "payload").await.unwrap_or_default();

                    let jobs = get_registered_jobs();
                    let mut handled = false;

                    for (_job_name, handler) in &jobs {
                        let job_result = handler(job_payload.clone()).await;

                        match job_result {
                            Ok(job) => {
                                if let Err(_) = job.before().await {
                                    let _: () = conn.hset_multiple(&job_key, &[
                                        ("status", "skipped"),
                                        ("skipped_at", &Utc::now().to_rfc3339()),
                                    ]).await.unwrap_or_default();
                                    break;
                                }

                                match job.perform().await {
                                    Ok(_) => {
                                        let _ = job.after().await;
                                        let _: () = conn.hset_multiple(&job_key, &[
                                            ("status", "success"),
                                            ("completed_at", &Utc::now().to_rfc3339()),
                                        ]).await.unwrap_or_default();
                                        let _: () = conn.incr("snm:qrush:success", 1).await.unwrap_or_default();
                                        // Track success jobs
                                        let _: () = conn.rpush(format!("snm:success:{}", queue), &job_id).await.unwrap_or_default();

                                        let key = format!("snm:stats:jobs:{}", today);
                                        // increment daily success counter
                                        let _: () = conn.incr(&key, 1).await.unwrap_or_default();
                                        let _: () = conn.incr("snm:qrush:total_jobs", 1).await.unwrap_or_default();

                                    }
                                    Err(err) => {
                                        let _ = job.on_error(&err).await;
                                        let retries: i64 = conn.hincr(&job_key, "retries", 1).await.unwrap_or(1);
                                        let _: () = conn.rpush(format!("snm:retry:{}", queue), &job_id).await.unwrap_or_default();
                                        if retries <= MAX_RETRIES as i64 {
                                            let backoff = 10 * retries;
                                            let now = Utc::now().timestamp();
                                            let _: () = conn.zadd(DELAYED_JOBS_KEY, &job_id, now + backoff).await.unwrap_or_default();
                                        }
                                    }
                                }

                                let _ = job.always().await;
                                handled = true;
                                break;
                            }
                            Err(e) => {
                                error_message = Some(e.to_string());
                            }
                        }
                    }

                    if !handled {
                        let mut hset_data = vec![
                            ("status", "failed".to_string()),
                            ("failed_at", Utc::now().to_rfc3339()),
                            ("queue", queue.clone()),
                            ("failed_at", Utc::now().to_rfc3339()),
                        ];

                        if let Some(ref emsg) = error_message {
                            hset_data.push(("error", emsg.clone()));
                        }

                        let fail_key = format!("snm:stats:jobs:{}:failed", today);
                        // increment daily failed counter
                        let _: () = conn.incr(&fail_key, 1).await.unwrap_or_default();


                        let _: () = conn.hset_multiple(&job_key, &hset_data).await.unwrap_or_default();
                        
                        let _: Result<(), _> = conn.lpush(
                            format!("snm:logs:{}", queue),
                            format!("[{}] ❌ Job {} failed", Utc::now(), job_id),
                        ).await;
                        let _: Result<(), _> = conn.ltrim(format!("snm:logs:{}", queue), 0, 99).await;
                        let _: () = conn.rpush("snm:failed_jobs", &job_id).await.unwrap_or_default();
                        let _: () = conn.hset(&job_key, "job_name", jobs.keys().next().unwrap_or(&"unknown")).await.unwrap_or_default();
                        // Track failed jobs
                        let _: () = conn.rpush(format!("snm:failed:{}", queue), &job_id).await.unwrap_or_default();
                        let _: () = conn.incr("snm:qrush:failed", 1).await.unwrap_or_default();
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
            let mut conn = match get_redis_connection().await {
                Ok(c) => c,
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            let jobs: Vec<String> = conn.zrangebyscore(DELAYED_JOBS_KEY, 0, now).await.unwrap_or_default();
            for job_str in jobs {
                let _: () = conn.lpush("snm:queue:default", &job_str).await.unwrap_or_default();
                let _: () = conn.zrem(DELAYED_JOBS_KEY, &job_str).await.unwrap_or_default();
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}



// src/config.rs

use std::sync::{Arc, OnceLock};
use crate::services::runner_service::{start_worker_pool, start_delayed_worker_pool};
use anyhow::{anyhow, Result};
use tokio::sync::Notify;
use tracing::info;
use redis::{aio::MultiplexedConnection, Client};
use redis::AsyncCommands;

#[derive(Clone, Debug)]
pub struct QueueConfig {
    pub name: String,
    pub concurrency: usize,
    pub priority: usize,
}


pub static QUEUE_INITIALIZED: OnceLock<Arc<Notify>> = OnceLock::new();
// Store Redis URL
pub static REDIS_URL: OnceLock<String> = OnceLock::new();
// Store global queues
pub static GLOBAL_QUEUES: OnceLock<Vec<QueueConfig>> = OnceLock::new();



pub static QRUSH_SHUTDOWN: OnceLock<Arc<Notify>> = OnceLock::new();

pub fn get_shutdown_notify() -> Arc<Notify> {
    QRUSH_SHUTDOWN.get_or_init(|| Arc::new(Notify::new())).clone()
}


async fn store_queue_metadata(queue: &QueueConfig) -> anyhow::Result<()> {
    let mut conn = get_redis_conn().await?;
    let redis_key = format!("snm:queue:config:{}", queue.name);
    conn.hset_multiple::<_, _, _, ()>(&redis_key, &[
        ("concurrency", queue.concurrency.to_string()),
        ("priority", queue.priority.to_string()),
    ]).await?;
    Ok(())
}


impl QueueConfig {
    pub fn new(name: impl Into<String>, concurrency: usize, priority: usize) -> Self {
        Self {
            name: name.into(),
            concurrency,
            priority,
        }
    }

    pub fn from_configs(configs: Vec<(&str, usize, usize)>) -> Vec<Self> {
        configs
            .into_iter()
            .map(|(name, concurrency, priority)| Self::new(name, concurrency, priority))
            .collect()
    }

    /// Initialize queue system with Redis and worker pools
    pub async fn initialize(redis_url: String, queues: Vec<Self>) -> Result<()> {
        set_redis_url(redis_url)?;
        set_global_queues(queues.clone())?;

        info!("Worker Pool Started");
        for queue in &queues {
            store_queue_metadata(queue).await?;
            // Store queue config in Redis for metrics
            let config_key = format!("snm:queue:config:{}", queue.name);
            let mut conn = get_redis_conn().await?;
            let _: () = redis::pipe()
                .hset(&config_key, "concurrency", queue.concurrency)
                .hset(&config_key, "priority", queue.priority)
                .query_async(&mut conn)
                .await?;
            start_worker_pool(&queue.name, queue.concurrency).await;
        }
        info!("Delayed Worker Pool Started");
        start_delayed_worker_pool().await;
        Ok(())
    }
}



pub fn get_global_queues() -> &'static [QueueConfig] {
    GLOBAL_QUEUES.get().expect("Queues not initialized")
}

pub fn set_global_queues(configs: Vec<QueueConfig>) -> Result<()> {
    GLOBAL_QUEUES
        .set(configs)
        .map_err(|_| anyhow!("Queues already initialized"))
}



pub fn get_redis_url() -> &'static str {
    REDIS_URL.get().expect("Redis URL is not set")
}

pub fn set_redis_url(url: String) -> Result<()> {
    REDIS_URL
        .set(url)
        .map_err(|_| anyhow!("Redis URL already set"))
}




#[derive(Debug, Clone)]
pub struct QrushBasicAuthConfig {
    pub username: String,
    pub password: String,
}

pub static QRUSH_BASIC_AUTH: OnceLock<Option<QrushBasicAuthConfig>> = OnceLock::new();

pub fn set_basic_auth(auth: Option<QrushBasicAuthConfig>) {
    let _ = QRUSH_BASIC_AUTH.set(auth);
}

pub fn get_basic_auth() -> Option<&'static QrushBasicAuthConfig> {
    QRUSH_BASIC_AUTH.get().and_then(|opt| opt.as_ref())
}





pub async fn get_redis_conn() -> redis::RedisResult<MultiplexedConnection> {
    let redis_url = get_redis_url();

    // Client::open will auto-handle rediss:// if TLS feature is enabled
    let client = Client::open(redis_url)?;
    client.get_multiplexed_async_connection().await
}

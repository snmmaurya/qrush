use std::sync::{Arc, OnceLock};
use crate::runner::{start_worker_pool, start_delayed_worker_pool};
use anyhow::{anyhow, Result};
use tokio::sync::Notify;
use tracing::{info, error, debug};


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

// src/job.rs
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

#[async_trait]
pub trait Job: Serialize + for<'de> Deserialize<'de> + Send + Sync + 'static {
    async fn perform(&self) -> anyhow::Result<()>;
    fn queue() -> &'static str;
    fn name() -> &'static str;
}




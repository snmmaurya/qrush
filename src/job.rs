// src/job.rs
use async_trait::async_trait;
use serde::{Serialize, Deserialize};


// src/job.rs
#[async_trait]
pub trait Job: Send + Sync {
    async fn before(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn perform(&self) -> anyhow::Result<()>;

    async fn after(&self) {}
    async fn on_error(&self, _err: &anyhow::Error) {}
    async fn always(&self) {}

    fn name() -> &'static str
    where
        Self: Sized;

    fn queue() -> &'static str
    where
        Self: Sized;
}




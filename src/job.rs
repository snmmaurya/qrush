// src/job.rs
use async_trait::async_trait;

#[async_trait]
pub trait Job: Send + Sync {
    async fn before(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn perform(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn after(&self) {}
    async fn on_error(&self, _err: &anyhow::Error) {}
    async fn always(&self) {}

    fn name(&self) -> &'static str;
    fn queue(&self) -> &'static str;
}

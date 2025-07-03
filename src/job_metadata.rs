// src/job_metadata.rs
use serde::{Serialize, de::DeserializeOwned};


pub trait JobMetadata: Serialize + DeserializeOwned {
    fn queue() -> &'static str;
    fn name() -> &'static str;
    fn max_retries() -> usize {
        0
    }
}

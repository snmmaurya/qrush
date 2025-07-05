// src/registry.rs

use std::collections::HashMap;
use std::sync::Mutex;
use futures::future::BoxFuture;
use once_cell::sync::Lazy;
use anyhow::Result;
use crate::job::Job;

/// Type alias for job handler functions.
/// These handlers take a serialized JSON `String` and return a boxed `Job` trait object.
pub type HandlerFn = fn(String) -> BoxFuture<'static, Result<Box<dyn Job>>>;

/// Global job registry holding job name → handler mappings.
pub static JOB_REGISTRY: Lazy<Mutex<HashMap<&'static str, HandlerFn>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Register a job type and its deserialization handler function.
pub fn register_job(name: &'static str, handler: HandlerFn) {
    JOB_REGISTRY.lock().unwrap().insert(name, handler);
}

/// Get a copy of the registered job handlers.
pub fn get_registered_jobs() -> HashMap<&'static str, HandlerFn> {
    JOB_REGISTRY.lock().unwrap().clone()
}

/// Try to get a handler from the registry for a given job type.
pub fn get_job_handler(name: &str) -> Option<HandlerFn> {
    JOB_REGISTRY.lock().unwrap().get(name).copied()
}

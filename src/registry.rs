// src/registry.rs
use std::collections::HashMap;
use futures::future::BoxFuture;
use std::sync::Mutex;
use once_cell::sync::Lazy;
use crate::job::Job;

pub type HandlerFn = fn(String) -> BoxFuture<'static, anyhow::Result<Box<dyn Job>>>;

static JOB_REGISTRY: Lazy<Mutex<HashMap<&'static str, HandlerFn>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_jobs(name: &'static str, handler: HandlerFn) {
    JOB_REGISTRY.lock().unwrap().insert(name, handler);
}

pub fn get_registered_jobs() -> HashMap<&'static str, HandlerFn> {
    JOB_REGISTRY.lock().unwrap().clone()
}

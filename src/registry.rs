use std::collections::HashMap;
use futures::future::BoxFuture;
use std::sync::Mutex;
use once_cell::sync::Lazy;

type HandlerFn = fn(String) -> BoxFuture<'static, anyhow::Result<()>>;

static JOB_REGISTRY: Lazy<Mutex<HashMap<&'static str, HandlerFn>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn register_jobs(name: &'static str, handler: HandlerFn) {
    JOB_REGISTRY.lock().unwrap().insert(name, handler);
}

pub fn get_registered_jobs() -> HashMap<&'static str, HandlerFn> {
    JOB_REGISTRY.lock().unwrap().clone()
}

# Qrush 🌀 – Lightweight Job Queue for Rust (Actix + Redis)

Qrush is a lightweight background job queue system for Rust, designed for Actix-Web projects with Redis as the backend. It supports job definition, scheduling (including delayed jobs), and monitoring with optional Basic Auth-protected Prometheus metrics.

---

## 🚀 Features

* Simple job definition using a trait
* Background job execution
* Delayed job scheduling (`enqueue_in`)
* Redis-based queue persistence
* Prometheus-style metrics endpoint (`/qrush/metrics`)
* Easy integration with Actix-Web
* Optional Basic Auth protection

---

## 📦 Installation

Add this to your `Cargo.toml`:

```toml
qrush = "0.1.0"
```

Or use:

```bash
cargo add qrush
```

---

## ⚙️ Integration Guide

### 1. Initialize Qrush

Create `src/jobs/qrush.rs`:

```rust
use std::sync::Arc;
use tokio::sync::Notify;
use qrush::config::{QueueConfig, set_basic_auth, QUEUE_INITIALIZED, QrushBasicAuthConfig};
use qrush::registry::register_jobs;
use qrush::job::Job;
use crate::jobs::workers::notify_user_job::NotifyUser;

pub async fn initiate(basic_auth: Option<QrushBasicAuthConfig>) {
    let queue_notify = Arc::new(Notify::new());
    let _ = set_basic_auth(basic_auth);
    let _ = QUEUE_INITIALIZED.set(queue_notify.clone());

    register_jobs(<NotifyUser as Job>::name(), NotifyUser::handler);

    tokio::spawn({
        let queue_notify = queue_notify.clone();
        async move {
            let redis_url = std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

            let queues = vec![
                QueueConfig::new("default", 5, 1),
                QueueConfig::new("critical", 10, 0),
            ];

            if let Err(err) = QueueConfig::initialize(redis_url, queues).await {
                eprintln!("Failed to initialize Qrush: {:?}", err);
            } else {
                println!("Qrush started successfully");
                queue_notify.notify_waiters();
            }
        }
    });

    queue_notify.notified().await;
}
```

---

### 2. Call `initiate()` in `main.rs`

```rust
use actix_web::{web, App, HttpServer};
use qrush::routes::metrics_route::qrush_metrics_routes;
use crate::jobs::qrush::initiate as qrush_initiate;
use qrush::config::QrushBasicAuthConfig;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let basic_auth = Some(QrushBasicAuthConfig {
        username: "qrush".into(),
        password: "qrush".into(),
    });

    qrush_initiate(basic_auth).await;

    HttpServer::new(|| {
        App::new()
            // Other routes...
            .service(web::scope("/qrush").configure(qrush_metrics_routes)) // Metrics route
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

---

### 3. Define a Job

Create `src/jobs/workers/notify_user_job.rs`:

```rust
use qrush::job::Job;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use futures::future::BoxFuture;

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyUser {
    pub user_id: String,
    pub message: String,
}

#[async_trait]
impl Job for NotifyUser {
    async fn perform(&self) -> Result<()> {
        println!("Performing NotifyUser: '{}' to user {}", self.message, self.user_id);
        Ok(())
    }

    fn name() -> &'static str {
        "NotifyUser"
    }

    fn queue() -> &'static str {
        "default"
    }
}

impl NotifyUser {
    pub fn handler(payload: String) -> BoxFuture<'static, Result<()>> {
        Box::pin(async move {
            let job: NotifyUser = serde_json::from_str(&payload)?;
            job.perform().await
        })
    }
}
```

---

### 4. Enqueue a Job Anywhere

```rust
use qrush::queue::{enqueue, enqueue_in};
use crate::jobs::workers::notify_user_job::NotifyUser;

// Fire immediately
let _ = enqueue(NotifyUser {
    user_id: "abc".into(),
    message: "Hello from Qrush!".into(),
}).await;

// Fire after 60 seconds
let _ = enqueue_in(NotifyUser {
    user_id: "abc".into(),
    message: "Delayed job after 60s".into(),
}, 60).await;
```

---

### 5. Metrics Route

Visit the metrics endpoint:

```
http://localhost:8080/qrush/metrics
```

---

### 6. Optional Environment Variables

| Variable    | Description             | Default                  |
| ----------- | ----------------------- | ------------------------ |
| `REDIS_URL` | Redis connection string | `redis://127.0.0.1:6379` |

---

## 💪 TODOs / Future Plans

* ✅ Retry logic and exponential backoff
* 🔄 Retryable job trait
* 💀 Dead letter queues
* 📜 Persistent job logs
* 🌐 Web UI for monitoring

---

## 👨‍💼 Author

Developed with ❤️ by **SNM Maurya**

---

## 🪪 License

MIT License

---

## 🔧 Optional Enhancements

Would you like to include any of the following in this README?

* [ ] CI/CD Badge (GitHub Actions, etc.)
* [ ] Docker integration instructions
* [ ] Logging with `tracing`
* [ ] Metrics with `prometheus` or `opentelemetry`

Let me know, and I’ll update the README accordingly.

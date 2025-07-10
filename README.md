# Qrush 🌀 – Lightweight Job Queue for Rust (Actix + Redis)

&#x20;&#x20;

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
qrush = "0.2.0"
```

Or use:

```bash
cargo add qrush
```

---

## ⚙️ Integration Guide for Version >= 0.2.0

### 1. Initialize Qrush

Create `src/jobs/qrush.rs`:

```rust
// src/jobs/qrush.rs

use std::sync::{Arc};
use tokio::sync::Notify;
use std::env;

use qrush::config::QueueConfig;
use qrush::registry::register_job;
use qrush::config::QUEUE_INITIALIZED;
use qrush::job::Job;
use qrush::config::{set_basic_auth, QrushBasicAuthConfig};
use crate::jobs::notify_user::NotifyUser;

pub async fn initiate(basic_auth: Option<QrushBasicAuthConfig>) {
    let queueNotify = Arc::new(Notify::new());
    let basic_auth = basic_auth.or_else(|| {
        std::env::var("QRUSH_BASIC_AUTH").ok().and_then(|auth| {
            let parts: Vec<&str> = auth.splitn(2, ':').collect();
            if parts.len() == 2 {
                Some(QrushBasicAuthConfig {
                    username: parts[0].to_string(),
                    password: parts[1].to_string(),
                })
            } else {
                None
            }
        })
    });

    let _ = set_basic_auth(basic_auth);

    let _ = QUEUE_INITIALIZED.set(queueNotify.clone());

    register_job(NotifyUser::name(), NotifyUser::handler);

    tokio::spawn({
        let queueNotify = queueNotify.clone();
        async move {
            let redis_url = std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

            let queues = vec![
                QueueConfig::new("default", 5, 1),
                QueueConfig::new("critical", 10, 0),
            ];

            if let Err(err) = QueueConfig::initialize(redis_url, queues).await {
                eprintln!("❌ Failed to initialize qrush: {:?}", err);
            } else {
                println!("✅ qrush started successfully");
                queueNotify.notify_waiters();
            }
        }
    });

    queueNotify.notified().await;
    println!("🚀 Queue initialization complete. Continuing main logic...");
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
            .service(web::scope("/qrush").configure(qrush_metrics_routes))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

---

### 3. Define a Job

Create `src/jobs/workers/notify_user.rs`:

```rust
use qrush::job::Job;
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use anyhow::{Result, Error};
use futures::future::{BoxFuture, FutureExt};

#[derive(Debug, Serialize, Deserialize)]
pub struct NotifyUser {
    pub user_id: String,
    pub message: String,
}

#[async_trait]
impl Job for NotifyUser {
    fn name(&self) -> &'static str {
        "NotifyUser"
    }

    fn queue(&self) -> &'static str {
        "default"
    }

    async fn before(&self) -> Result<()> {
        println!("⏳ Before NotifyUser job for user: {}", self.user_id);
        Ok(())
    }

    async fn perform(&self) -> Result<()> {
        println!("📬 Performing NotifyUser: '{}' to user {}", self.message, self.user_id);
        Ok(())
    }

    async fn after(&self) {
        println!("✅ After NotifyUser job for user: {}", self.user_id);
    }

    async fn on_error(&self, err: &Error) {
        eprintln!("❌ Error in NotifyUser job for user {}: {:?}", self.user_id, err);
    }

    async fn always(&self) {
        println!("🔁 Always block executed for NotifyUser job");
    }
}

impl NotifyUser {
    pub fn name() -> &'static str {
        "notify_user"
    }

    pub fn handler(payload: String) -> BoxFuture<'static, Result<Box<dyn Job>>> {
        async move {
            let job: NotifyUser = serde_json::from_str(&payload)?;
            Ok(Box::new(job) as Box<dyn Job>)
        }.boxed()
    }
}
```

---

### 4. Enqueue a Job Anywhere

```rust
use qrush::queue::{enqueue, enqueue_in};
use crate::jobs::notify_user::NotifyUser;

let _ = enqueue(NotifyUser {
    user_id: "abc".into(),
    message: "Hello from Qrush!".into(),
}).await;

let _ = enqueue_in(NotifyUser {
    user_id: "abc".into(),
    message: "Delayed job after 60s".into(),
}, 60).await;
```

---

### 5. Metrics Route

Visit:

```
http://localhost:8080/qrush/metrics
```

---

## ⚙️ Integration Guide for Version <= 0.1.2

### 1. Initialize Qrush (Legacy)

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
            .service(web::scope("/qrush").configure(qrush_metrics_routes))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
```

### 3. Define a Job (Legacy Style)

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

### 4. Enqueue a Job

```rust
use qrush::queue::{enqueue, enqueue_in};
use crate::jobs::workers::notify_user_job::NotifyUser;

let _ = enqueue(NotifyUser {
    user_id: "abc".into(),
    message: "Hello from Qrush!".into(),
}).await;

let _ = enqueue_in(NotifyUser {
    user_id: "abc".into(),
    message: "Delayed job after 60s".into(),
}, 60).await;
```

### 5. Metrics Route

Visit:

```
http://localhost:8080/qrush/metrics
```

---

## 📈 Optional Environment Variables

| Variable           | Description               | Default                  |
| ------------------ | ------------------------- | ------------------------ |
| `REDIS_URL`        | Redis connection string   | `redis://127.0.0.1:6379` |
| `QRUSH_BASIC_AUTH` | Basic Auth as `user:pass` | *Optional*               |

---

## 💪 TODO / Roadmap

* 💀 Dead Letter Queues
* 📜 Persistent Job Logs
* 🌐 Web UI for Monitoring

---

## 👨‍💼 Author

Built with ❤️ by **[SNM Maurya](https://snmmaurya.com/solutions/qrush)**

---

## 🖼️ Screenshots

### Qrush Metrics Example

[https://snmmaurya.com/qrush/metrics.png](https://snmmaurya.com/qrush/metrics.png)

[https://snmmaurya.com/qrush/summary.png](https://snmmaurya.com/qrush/summary.png)

---

## 🪪 License

MIT

---

## 🔧 Optional Enhancements

*

Feel free to open issues or PRs with feature requests.

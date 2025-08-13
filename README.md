![QRush](https://snmmaurya.s3.ap-south-1.amazonaws.com/qrush.png)
# QRush Integration Guide (Actix Web)

This guide shows how to integrate **QRush** into an Actix Web application — exactly like your test app:
- Redis-backed queue
- Workers & delayed mover
- Jobs with `Job` trait
- Metrics UI mounted at **`/qrush/metrics`**
- Optional Basic Auth for the metrics panel
- Seconds-based scheduling (`enqueue_in(job, delay_secs)`)

---

## 1) Add dependencies

```toml
# Cargo.toml
[dependencies]
actix-web = "4"
dotenv = "0.15"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
serde = { version = "1", features = ["derive"] }
anyhow = "1"
async-trait = "0.1"

# the queue
qrush = "<latest>"
```

---

## 2) Environment variables

```bash
# REQUIRED Redis connection
export REDIS_URL="redis://127.0.0.1:6379"

# REQUIRED Basic Auth for metrics (username:password)
export QRUSH_BASIC_AUTH="admin:strongpassword"

# Optional: server bind (used by your test app)
export SERVER_ADDRESS="0.0.0.0:8080"
```

---

## 3) Define a job

Implement the `Job` trait for your payload. Keep `name()` and `queue()` consistent with registration.

```rust
// /test/src/jobs/notify_user.rs
use qrush::job::Job;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct NotifyUser {
    pub user_id: String,
    pub message: String,
}

#[async_trait]
impl Job for NotifyUser {
    async fn perform(&self) -> anyhow::Result<()> {
        println!("Notify {} -> {}", self.user_id, self.message);
        Ok(())
    }

    fn name(&self) -> &'static str { "NotifyUser" }
    fn queue(&self) -> &'static str { "default" }
}
```

---

## 4) QRush initializer (as in your test app)

This sets optional Basic Auth, registers jobs, initializes queues in a background task, then waits until ready.

```rust
// /test/src/jobs/qrush.rs
use actix_web::web;
use std::sync::Arc;
use tokio::sync::Notify;
use qrush::config::{QueueConfig, QUEUE_INITIALIZED, set_basic_auth, QrushBasicAuthConfig};
use qrush::registry::register_job;
use crate::jobs::notify_user::NotifyUser;
use qrush::routes::metrics_route::qrush_metrics_routes;

pub struct QrushInitializer;

impl QrushInitializer {
    pub async fn initialize(basic_auth: Option<QrushBasicAuthConfig>) {
        let queue_notify = Arc::new(Notify::new());

        // Basic Auth from param or env (QRUSH_BASIC_AUTH="user:pass")
        let basic_auth = basic_auth.or_else(|| {
            std::env::var("QRUSH_BASIC_AUTH").ok().and_then(|auth| {
                let parts: Vec<&str> = auth.splitn(2, ':').collect();
                (parts.len() == 2).then(|| QrushBasicAuthConfig {
                    username: parts[0].to_string(),
                    password: parts[1].to_string(),
                })
            })
        });
        let _ = set_basic_auth(basic_auth);

        // Expose the notify handle if other parts need it
        let _ = QUEUE_INITIALIZED.set(queue_notify.clone());

        // Register job handlers
        register_job(NotifyUser::name(), NotifyUser::handler);

        // Initialize queues in the background
        tokio::spawn({
            let queue_notify = queue_notify.clone();
            async move {
                let redis_url = std::env::var("REDIS_URL")
                    .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

                // queue_name, concurrency, priority
                let queues = vec![
                    QueueConfig::new("default", 5, 1),
                    QueueConfig::new("critical", 10, 0),
                ];

                if let Err(err) = QueueConfig::initialize(redis_url, queues).await {
                    eprintln!("❌ Failed to initialize qrush: {:?}", err);
                } else {
                    println!("✅ qrush started successfully");
                    queue_notify.notify_waiters();
                }
            }
        });

        // Wait until init completes
        queue_notify.notified().await;
        println!("🚀 Queue initialization complete. Continuing main logic...");
    }

    pub fn get_qrush_metrics_routes(cfg: &mut web::ServiceConfig) {
        qrush_metrics_routes(cfg)
    }
}
```

---

## 5) Wire it into `main.rs`

- Initialize QRush (awaits until queues are ready).
- Mount metrics under `/qrush/metrics`.
- Seed a few jobs to verify the dashboard.

```rust
// /test/src/main.rs
use actix_web::{web, App, HttpServer, HttpResponse, Responder, middleware::Logger};
use dotenv::dotenv;
use std::env;
use crate::jobs::qrush::QrushInitializer;
use qrush::queue::{enqueue, enqueue_in};
use crate::jobs::notify_user::NotifyUser;

async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("Test App Backend is Running!")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    // Initialize QRush (wait until queues are up)
    QrushInitializer::initialize(None).await;

    // Seed some jobs
    let _ = enqueue(NotifyUser {
        user_id: "abc".into(),
        message: "Hello from Qrush!".into(),
    }).await;

    let _ = enqueue_in(NotifyUser {
        user_id: "abc".into(),
        message: "Delayed job after 60s".into(),
    }, 60).await;

    let _ = enqueue_in(NotifyUser {
        user_id: "abc".into(),
        message: "Delayed job after 120s".into(),
    }, 120).await;

    let _ = enqueue_in(NotifyUser {
        user_id: "abc".into(),
        message: "Delayed job after 180s".into(),
    }, 180).await;

    // Bind address
    let server_address = env::var("SERVER_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            // Metrics UI
            .service(
                web::scope("/qrush")
                    .configure(|cfg| QrushInitializer::get_qrush_metrics_routes(cfg))
            )
            .route("/", web::get().to(health_check))
            .route("/health", web::get().to(health_check))
    })
    .bind(server_address)?
    .run()
    .await
}
```

---

## 6) Enqueue from anywhere

```rust
use qrush::queue::{enqueue, enqueue_in};

let _ = enqueue(NotifyUser {
    user_id: "u-1".into(),
    message: "Immediate",
}).await;

let _ = enqueue_in(NotifyUser {
    user_id: "u-2".into(),
    message: "Later",
}, 300).await; // in 300 seconds
```

---

## 7) Run

```bash
# Start Redis
redis-server        # or: docker run -p 6379:6379 redis:7

# Run your service
cargo run
```

Open:
- **Dashboard**: `http://localhost:8080/qrush/metrics`
- **Per-queue**: `http://localhost:8080/qrush/metrics/queues/default`
- **Health**: `http://localhost:8080/qrush/metrics/health`

If you set `QRUSH_BASIC_AUTH`, include Basic auth in requests:
```bash
curl -u admin:strongpassword http://localhost:8080/qrush/metrics/health
```

---

## 8) Endpoints (under `/qrush/metrics`)

- `GET /` — queues overview (success / failed / retry / pending)
- `GET /queues/{queue}` — jobs in a queue
- `GET /queues/{queue}/export` — CSV export
- `GET /extras/delayed` — delayed jobs
- `GET /extras/dead` — dead jobs
- `GET /extras/scheduled` — scheduled jobs
- `GET /extras/workers` — worker status
- `GET /health` — health probe
- `POST /jobs/action` — job actions (`{"action":"retry|delete|queue","job_id":"..."}`)

---

## Notes & tips

- **Scheduling**: `enqueue_in(job, delay_secs)` uses seconds (integer), matching your test app.
- **QueueConfig**: you used two queues (`default`, `critical`) with different concurrency/priority; tune as needed.
- **Register jobs before init**: ensure `register_job(name, handler)` runs before workers start.
- **Templates**: metrics UI uses Tera templates. If a Tera parse error occurs, restart the process (once_cell poison).
- **Security**: protect metrics with Basic Auth via `QRUSH_BASIC_AUTH` or your own middleware.

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🙏 Acknowledgments

- Built with [Actix Web](https://actix.rs/) - Fast, powerful web framework
- UI powered by [TailwindCSS](https://tailwindcss.com/) - Utility-first CSS framework


## 📞 Support

- 📖 [How it works](https://qrush.snmmaurya.com/get-started)
- 📖 [Support](https://qrush.snmmaurya.com/support)
- 📖 [Documentation](https://docs.rs/qrush)
- 💬 [Discussions](https://github.com/snmmaurya/qrush/discussions)
- 🐛 [Issues](https://github.com/snmmaurya/qrush/issues)
- 📧 Email: sxmmaurya@gmail.com

---

**Made with ❤️ by the rustacean360 (Snm Maurya)**

[![GitHub stars](https://img.shields.io/github/stars/snmmaurya/qrush?style=social)](https://github.com/snmmaurya/qrush)
[![LinkedIn Follow](https://img.shields.io/badge/LinkedIn-Follow-blue?style=social&logo=linkedin)](https://www.linkedin.com/company/rustacean360)

![QRush](https://snmmaurya.s3.ap-south-1.amazonaws.com/qrush.png)

# QRush Integration Guide (Actix Web)

This guide shows how to integrate **QRush** into an Actix Web application — exactly like your test app:
- Redis-backed queue
- Workers & delayed mover
- Jobs with `Job` trait
- Cron scheduling with `CronJob` trait
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
qrush = "0.5.0"
```

---

## 2) Environment variables

```bash
# REQUIRED Redis connection
export REDIS_URL="redis://127.0.0.1:6379"

# OPTIONAL Basic Auth for metrics (username:password)
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
        // YOUR NOTIFY USER LOGIC GOES HERE
        println!("Notify {} -> {}", self.user_id, self.message);
        Ok(())
    }

    fn name(&self) -> &'static str { "NotifyUser" }
    fn queue(&self) -> &'static str { "default" }
}

impl NotifyUser {
    pub fn name() -> &'static str { "NotifyUser" }

    pub fn handler(payload: String) -> BoxFuture<'static, Result<Box<dyn Job>>> {
        Box::pin(async move {
            let job: NotifyUser = serde_json::from_str(&payload)?;
            Ok(Box::new(job) as Box<dyn Job>)
        })
    }
}
```

---

## 4) QRush initializer (as in your test app)

This sets optional Basic Auth, registers jobs, initializes queues in a background task, then waits until ready.

```rust
// /test/src/qrushes/qrush_integrated.rs
use actix_web::web;
use std::sync::Arc;
use tokio::sync::{Notify, OnceCell};
use qrush::config::{QueueConfig, QUEUE_INITIALIZED, set_basic_auth, QrushBasicAuthConfig};
use qrush::registry::register_job;
use qrush::cron::cron_scheduler::CronScheduler;
use qrush::routes::metrics_route::qrush_metrics_routes;
use crate::qrushes::jobs::notify_user::NotifyUser;
use crate::qrushes::crons::daily_report_job::DailyReportJob;

static QRUSH_INTEGRATED_INIT: OnceCell<Arc<Notify>> = OnceCell::const_new();

pub struct QrushIntegrated;

impl QrushIntegrated {
    pub async fn initialize(basic_auth: Option<QrushBasicAuthConfig>) {
        if let Some(existing_notify) = QRUSH_INTEGRATED_INIT.get() {
            existing_notify.notified().await;
            return;
        }

        let queue_notify = Arc::new(Notify::new());
        let _ = QRUSH_INTEGRATED_INIT.set(queue_notify.clone());

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
        let _ = QUEUE_INITIALIZED.set(queue_notify.clone());

        // Register jobs globally
        register_job(NotifyUser::name(), NotifyUser::handler);
        register_job(DailyReportJob::name(), DailyReportJob::handler);

        // Initialize queues in background
        tokio::spawn({
            let queue_notify = queue_notify.clone();
            async move {
                let redis_url = std::env::var("REDIS_URL")
                    .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

                let queues = vec![
                    QueueConfig::new("default", 5, 1),
                    QueueConfig::new("critical", 10, 0),
                    QueueConfig::new("integrated", 3, 2),
                ];

                if let Err(err) = QueueConfig::initialize(redis_url, queues).await {
                    eprintln!("Failed to initialize qrush: {:?}", err);
                } else {
                    println!("QRush queues started successfully");
                }
                queue_notify.notify_waiters();
            }
        });

        // Wait for queue initialization
        queue_notify.notified().await;
        
        // Register cron jobs after queues are ready
        Self::register_cron_jobs().await;
    }

    async fn register_cron_jobs() {
        let daily_report_job = DailyReportJob {
            report_type: "integrated_report".to_string(),
        };
        
        if let Err(e) = CronScheduler::register_cron_job(daily_report_job).await {
            println!("Failed to register cron job: {:?}", e);
        }
    }

    pub fn configure_routes(cfg: &mut web::ServiceConfig) {
        qrush_metrics_routes(cfg);
    }

    pub fn setup_worker_sync() -> QrushWorkerConfig {
        // Worker setup logic for debugging/monitoring
        QrushWorkerConfig {
            worker_id: nanoid::nanoid!(),
            initialized_at: std::time::SystemTime::now(),
            integration_mode: "integrated".to_string(),
        }
    }
}
```

---

## 5) Define cron jobs (optional)

For recurring scheduled jobs, implement both `Job` and `CronJob` traits:

```rust
// /test/src/qrushes/crons/daily_report_job.rs
use async_trait::async_trait;
use qrush::job::Job;
use qrush::cron::cron_job::CronJob;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DailyReportJob {
    pub report_type: String,
}

#[async_trait]
impl Job for DailyReportJob {
    async fn perform(&self) -> anyhow::Result<()> {
        println!("Generating {} report", self.report_type);
        // YOUR REPORT GENERATION LOGIC
        Ok(())
    }

    fn name(&self) -> &'static str { "DailyReportJob" }
    fn queue(&self) -> &'static str { "default" }
}

#[async_trait]
impl CronJob for DailyReportJob {
    fn cron_expression(&self) -> &'static str {
        "0 * * * * *"  // Every minute (for testing)
    }

    fn cron_id(&self) -> &'static str { "daily_report" }
}

impl DailyReportJob {
    pub fn name() -> &'static str { "DailyReportJob" }

    pub fn handler(payload: String) -> BoxFuture<'static, Result<Box<dyn Job>>> {
        Box::pin(async move {
            let job: DailyReportJob = serde_json::from_str(&payload)?;
            Ok(Box::new(job) as Box<dyn Job>)
        })
    }
}
```

---

## 6) Wire it into `main.rs`

- Initialize QRush (awaits until queues are ready).
- Mount metrics under `/qrush/metrics`.
- Seed a few jobs to verify the dashboard.

```rust
// /test/src/main.rs
use actix_web::{web, App, HttpServer, HttpResponse, Responder, middleware::Logger};
use dotenv::dotenv;
use std::env;
use crate::qrushes::qrush_integrated::QrushIntegrated;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    QrushIntegrated::initialize(None).await;
    println!("QRush initialization complete!");

    let server_address = env::var("SERVER_ADDRESS")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    HttpServer::new(move || {
        // Worker-specific setup per app instance
        let qrush_worker_config = QrushIntegrated::setup_worker_sync();
        
        App::new()
            .app_data(web::Data::new(qrush_worker_config))
            .wrap(Logger::default())
            // QRush metrics routes
            .service(
                web::scope("/qrush")
                    .configure(QrushIntegrated::configure_routes)
            )
    })
    .bind(server_address)?
    .run()
    .await
}
```

---

## 7) Enqueue from anywhere

```rust
use qrush::queue::{enqueue, enqueue_in};

let _ = enqueue(NotifyUser {
    user_id: "u-1".into(),
    message: "Immediate".into(),
}).await;

let _ = enqueue_in(NotifyUser {
    user_id: "u-2".into(),
    message: "Later".into(),
}, 300).await; // in 300 seconds
```

---

## 8) Run

```bash
# Start Redis
redis-server        # or: docker run -p 6379:6379 redis:7

# Run your service
cargo run
```

Open:
- **Dashboard**: `http://localhost:8080/qrush/metrics`
- **Per-queue**: `http://localhost:8080/qrush/metrics/queues/default`
- **Cron Jobs**: `http://localhost:8080/qrush/metrics/extras/cron`
- **Health**: `http://localhost:8080/qrush/metrics/health`

If you set `QRUSH_BASIC_AUTH`, include Basic auth in requests:
```bash
curl -u admin:strongpassword http://localhost:8080/qrush/metrics/health
```

---

## 9) Endpoints (under `/qrush/metrics`)

- `GET /` — queues overview (success / failed / retry / pending)
- `GET /queues/{queue}` — jobs in a queue
- `GET /queues/{queue}/export` — CSV export
- `GET /extras/delayed` — delayed jobs
- `GET /extras/dead` — dead jobs
- `GET /extras/scheduled` — scheduled jobs
- `GET /extras/cron` — cron jobs management
- `GET /extras/workers` — worker status
- `GET /extras/summary` — metrics summary with charts
- `GET /health` — health probe
- `POST /jobs/action` — job actions (`{"action":"retry|delete|queue","job_id":"..."}`)
- `POST /cron/action` — cron actions (`{"action":"toggle|delete|run_now","job_id":"..."}`)

---

## 10) CLI Usage (Optional)

Build the CLI tool for advanced management:

```bash
cargo build --release --bin qrush
```

### Basic Commands

```bash
# Start workers
qrush start -q critical,default -c 10

# Check status  
qrush status -v

# View queue statistics
qrush stats -w

# List jobs
qrush jobs list -q default -s pending

# Retry failed jobs
qrush jobs retry --all

# Manage cron jobs
qrush cron list
qrush cron enable daily_report

# Start web UI
qrush web -p 4567
```

---

## Common Cron Expressions

- `"0 * * * * *"` - Every minute
- `"0 */5 * * * *"` - Every 5 minutes  
- `"0 0 * * * *"` - Every hour
- `"0 0 0 * * *"` - Daily at midnight
- `"0 0 0 * * 1"` - Every Monday
- `"0 0 0 1 * *"` - First day of month

---

## Notes & tips

- **Scheduling**: `enqueue_in(job, delay_secs)` uses seconds (integer), matching your test app.
- **QueueConfig**: you used three queues (`default`, `critical`, `integrated`) with different concurrency/priority; tune as needed.
- **CronJobs**: implement both `Job` and `CronJob` traits, register with `CronScheduler::register_cron_job()` after queue initialization, supports standard cron expressions with 6-field format (sec min hour day month weekday).
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

## Support

- **Documentation**: [docs.rs/qrush](https://docs.rs/qrush)
- **Issues**: [GitHub Issues](https://github.com/snmmaurya/qrush/issues)
- **Discussions**: [GitHub Discussions](https://github.com/snmmaurya/qrush/discussions)
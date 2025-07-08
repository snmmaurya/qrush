// // src/routes/metrics_route.rs

use actix_web::{web, HttpResponse};
use crate::services::metrics_service::{
    render_metrics,
    render_metrics_for_queue,
    render_dead_jobs,
    render_scheduled_jobs,
    render_delayed_jobs,
    render_worker_status,
    job_action,
    export_queue_csv,
    get_metrics_summary,
};

pub fn qrush_metrics_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("metrics")
            .route("/health", web::get().to(|| async {
                HttpResponse::Ok().body("healthy")
            }))
            .route("", web::get().to(render_metrics))
            .route("/queues/{queue}", web::get().to(render_metrics_for_queue))
            .route("/extras/delayed", web::get().to(render_delayed_jobs))
            .route("/extras/dead", web::get().to(render_dead_jobs))
            .route("/extras/scheduled", web::get().to(render_scheduled_jobs))
            .route("/extras/workers", web::get().to(render_worker_status))
            .route("/queues/{queue}/export", web::get().to(export_queue_csv))
            .route("/extras/summary", web::get().to(get_metrics_summary))
            .route("/jobs/action", web::post().to(job_action))
    );
}

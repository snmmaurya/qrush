use actix_web::{web, HttpResponse};
use crate::services::metrics_service::{
    render_metrics,
    render_metrics_for_queue,
    job_action,
    export_queue_csv,
    retry_job,
    get_metrics_summary,
};

pub fn qrush_metrics_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .route("/health", web::get().to(|| async {
                HttpResponse::Ok().body("healthy")
            }))
            .route("/metrics", web::get().to(render_metrics))
            .route("/metrics/{queue}", web::get().to(render_metrics_for_queue))
            .route("/queue/{queue}", web::get().to(render_metrics))
            .route("/queue/{queue}/export", web::get().to(export_queue_csv))
            .route("/summary", web::get().to(get_metrics_summary))
            .route("/job/action", web::post().to(job_action))
            .route("/job/retry", web::post().to(retry_job))
    );
}
use actix_web::web;
use crate::services::metrics_service::{metrics, view_queue, job_action};

pub fn qrush_metrics_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .route("/metrics", web::get().to(metrics))         
            .route("/queue/{queue}", web::get().to(view_queue))
            .route("/job_action", web::post().to(job_action))
    );
}

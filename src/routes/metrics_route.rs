use actix_web::{web};
use crate::services::metrics_service::{metrics, retry};

pub fn qrush_metrics_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("")
            .route("/metrics", web::get().to(metrics))  // final route: /qrush/metrics
            .route("/retry", web::get().to(retry))      // final route: /qrush/retry
    );
}
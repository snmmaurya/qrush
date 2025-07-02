// src/services/metrics_service.rs

use actix_web::{post, HttpRequest, HttpResponse, Responder, web};
use serde::Deserialize;
use crate::basic_auth::{check_basic_auth, unauthorized_response};
use crate::redis_pool::get_redis_conn;
use redis::AsyncCommands;
use crate::dashboard::metrics::{render_metrics, render_queue_jobs};
use crate::config::get_basic_auth;


#[derive(Deserialize)]
pub struct MetricsParams {
    page: Option<usize>,
}


#[derive(Deserialize)]
pub struct RetryParams {
    queue: String,
}


pub async fn metrics(req: HttpRequest, query: web::Query<MetricsParams>) -> impl Responder {
    // Basic Auth check
    if get_basic_auth().is_some() && !check_basic_auth(&req) {
        return unauthorized_response();
    }

    let page = query.page.unwrap_or(1);

    match render_metrics(page).await {
        Ok((html, _, _)) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed: {}", e)),
    }
}




pub async fn view_queue(
    req: HttpRequest,
    path: web::Path<String>,
) -> impl Responder {
    if get_basic_auth().is_some() && !check_basic_auth(&req) {
        return unauthorized_response();
    }

    let queue = path.into_inner();

    match render_queue_jobs(&queue).await {
        Ok((_, html)) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(e) => HttpResponse::InternalServerError().body(format!("Failed: {}", e)),
    }
}



#[derive(Deserialize)]
pub struct JobActionParams {
    job_id: String,
    action: String, // retry, run, delete
    queue: String,
}

/// Retry / Run / Delete Job
pub async fn job_action(
    req: HttpRequest,
    form: web::Form<JobActionParams>,
) -> impl Responder {
    if get_basic_auth().is_some() && !check_basic_auth(&req) {
        return unauthorized_response();
    }

    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let job_key = format!("snm:job:{}", form.job_id);
    let queue_key = format!("snm:queue:{}", form.queue);

    match form.action.as_str() {
        "retry" | "run" => {
            let _: () = conn.lpush(&queue_key, &form.job_id).await.unwrap_or_default();
            let _: () = conn.zrem("snm:delayed_jobs", &form.job_id).await.unwrap_or_default();
            let _: () = conn.hset(&job_key, "status", "pending").await.unwrap_or_default();
            HttpResponse::Ok().body("Job moved to queue")
        }
        "delete" => {
            let _: () = conn.del(&job_key).await.unwrap_or_default();
            let _: () = conn.lrem::<_, _, ()>(&queue_key, 0, &form.job_id).await.unwrap_or_default();
            let _: () = conn.zrem("snm:delayed_jobs", &form.job_id).await.unwrap_or_default();
            HttpResponse::Ok().body("Job deleted")
        }
        _ => HttpResponse::BadRequest().body("Unknown action"),
    }
}

use actix_web::{post, HttpRequest, HttpResponse, Responder, web};
use serde::Deserialize;
use crate::basic_auth::{check_basic_auth, unauthorized_response};
use crate::redis_pool::get_redis_conn;
use redis::AsyncCommands;
use crate::dashboard::metrics::render_metrics;
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


pub async fn retry(req: HttpRequest, query: web::Query<RetryParams>) -> impl Responder {
    // Basic Auth check
    if get_basic_auth().is_some() && !check_basic_auth(&req) {
        return unauthorized_response();
    }

    let queue = &query.queue;
    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let delayed_key = "snm:delayed_jobs";
    let queue_key = format!("snm:queue:{}", queue);
    // Move all delayed jobs for this queue (matching by prefix)
    let now = chrono::Utc::now().timestamp_millis();
    let jobs: Vec<String> = match conn.zrangebyscore(delayed_key, 0, now).await {
        Ok(j) => j,
        Err(_) => vec![],
    };

    let mut retried = 0;
    for job in jobs {
        if job.contains(&format!("\"queue\":\"{}\"", queue)) {
            if conn.lpush::<_, _, ()>(&queue_key, &job).await.ok().is_some() {
                let _: () = conn.zrem(delayed_key, &job).await.unwrap_or_default();
                retried += 1;
            }
        }
    }

    HttpResponse::Ok().body(format!("✅ Retried {} jobs in queue: {}", retried, queue))
}

// src/services/metrics_service.rs
use std::collections::HashMap;
use actix_web::{web, HttpResponse, Responder, http::header::ContentDisposition};
use actix_web::web::Data;
use tera::{Context, Tera};
use redis::AsyncCommands;
use lazy_static::lazy_static;
use csv::WriterBuilder;
use serde::Deserialize;
use serde_json::json;
use chrono::Utc;
use serde::Serialize;

use crate::rdconfig::get_redis_conn;
use crate::constants::DELAYED_JOBS_KEY;

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = Tera::default();

        tera.add_raw_template("metrics.html.tera", include_str!("../templates/metrics.html.tera"))
            .expect("Failed to add metrics.html.tera");

        println!("Tera templates embedded at compile-time");
        tera
    };
}

pub async fn render_metrics() -> impl Responder {
    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let success: i64 = conn.get("snm:qrush:success").await.unwrap_or(0);
    let failed: i64 = conn.get("snm:qrush:failed").await.unwrap_or(0);
    let retried = failed - success;
    let delayed: i64 = conn.zcard(DELAYED_JOBS_KEY).await.unwrap_or(0);
    let queued: i64 = conn.llen("snm:queue:default").await.unwrap_or(0);

    let keys: Vec<String> = conn.keys::<&str, Vec<String>>("snm:queue:*").await.unwrap_or_default();
    let all_queues: Vec<String> = keys.iter()
        .filter_map(|k| k.strip_prefix("snm:queue:").map(|s| s.to_string()))
        .collect();

    let selected_queue = "default".to_string();
    let job_ids: Vec<String> = conn.lrange("snm:queue:default", 0, 50).await.unwrap_or_default();

    let mut jobs = Vec::new();
    for job_id in &job_ids {
        let job_key = format!("snm:job:{job_id}");
        let meta: redis::RedisResult<HashMap<String, String>> = conn.hgetall(&job_key).await;
        if let Ok(m) = meta {
            jobs.push((job_id.clone(), m));
        }
    }

    let mut ctx = Context::new();
    ctx.insert("success", &success);
    ctx.insert("failed", &failed);
    ctx.insert("retried", &retried);
    ctx.insert("delayed", &delayed);
    ctx.insert("queued", &queued);
    ctx.insert("selected_queue", &selected_queue);
    ctx.insert("queues", &all_queues);
    ctx.insert("jobs", &jobs);

    match TEMPLATES.render("metrics.html.tera", &ctx) {
        Ok(html) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(e) => HttpResponse::InternalServerError().body(format!("Template error: {}", e)),
    }
}

pub async fn render_metrics_for_queue(path: web::Path<String>) -> impl Responder {
    let queue = path.into_inner();
    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let job_ids: Vec<String> = conn
        .lrange(format!("snm:queue:{queue}"), 0, 50)
        .await
        .unwrap_or_default();

    let mut jobs = Vec::new();
    for job_id in &job_ids {
        let job_key = format!("snm:job:{job_id}");
        let meta: redis::RedisResult<HashMap<String, String>> = conn.hgetall(&job_key).await;
        if let Ok(m) = meta {
            jobs.push((job_id.clone(), m));
        }
    }

    let keys: Vec<String> = conn.keys::<&str, Vec<String>>("snm:queue:*").await.unwrap_or_default();
    let all_queues: Vec<String> = keys.iter()
        .filter_map(|k| k.strip_prefix("snm:queue:").map(|s| s.to_string()))
        .collect();

    let mut ctx = Context::new();
    ctx.insert("selected_queue", &queue);
    ctx.insert("queues", &all_queues);
    ctx.insert("jobs", &jobs);

    match TEMPLATES.render("metrics.html.tera", &ctx) {
        Ok(html) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(e) => HttpResponse::InternalServerError().body(format!("Template error: {}", e)),
    }
}

#[derive(Deserialize)]
pub struct JobAction {
    pub job_id: String,
    pub action: String, // "run" or "delete"
}

pub async fn job_action(form: web::Form<JobAction>) -> impl Responder {
    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let job_id = &form.job_id;
    let job_key = format!("snm:job:{}", job_id);
    let queue_key: String = conn
        .hget(&job_key, "queue")
        .await
        .unwrap_or_else(|_| "snm:queue:default".to_string());

    match form.action.as_str() {
        "run" => {
            let _: () = conn.lpush::<_, _, ()>(&queue_key, job_id).await.unwrap_or_default();
        }
        "delete" => {
            let _: () = conn.del::<_, ()>(&job_key).await.unwrap_or_default();
            let _: () = conn.lrem::<_, _, ()>(&queue_key, 0, job_id).await.unwrap_or_default();
        }
        _ => {}
    }

    HttpResponse::Ok().finish()
}



pub async fn export_queue_csv(path: web::Path<String>) -> actix_web::Result<HttpResponse> {
    let queue = path.into_inner();
    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => {
            return Ok(HttpResponse::InternalServerError()
                .body("Failed to connect to Redis"));
        }
    };

    let job_ids: Vec<String> = match conn.lrange(format!("snm:queue:{queue}"), 0, -1).await {
        Ok(ids) => ids,
        Err(_) => {
            return Ok(HttpResponse::InternalServerError()
                .body("Failed to fetch jobs from Redis"));
        }
    };

    let mut wtr = WriterBuilder::new().from_writer(vec![]);
    wtr.write_record(&["job_id", "status", "created_at", "payload"])
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    for job_id in &job_ids {
        let job_key = format!("snm:job:{job_id}");
        let payload: String = conn.hget(&job_key, "payload").await.unwrap_or_default();
        let status: String = conn.hget(&job_key, "status").await.unwrap_or_default();
        let created_at: String = conn.hget(&job_key, "created_at").await.unwrap_or_default();

        wtr.write_record(&[job_id, &status, &created_at, &payload])
            .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;
    }

    let data = wtr
        .into_inner()
        .map_err(|e| actix_web::error::ErrorInternalServerError(e))?;

    Ok(HttpResponse::Ok()
        .content_type("text/csv")
        .insert_header(ContentDisposition::attachment("export.csv"))
        .body(data))
}




#[derive(Deserialize)]
pub struct RetryRequest {
    pub job_id: String,
    pub queue_name: String,
}

pub async fn retry_job(req: web::Json<RetryRequest>) -> Result<HttpResponse, actix_web::Error> {
    let RetryRequest { job_id, queue_name } = req.into_inner();
    let mut conn = get_redis_conn().await.map_err(actix_web::error::ErrorInternalServerError)?;

    let job_key = format!("snm:job:{}", job_id);
    let queue_key = format!("snm:queue:{}", queue_name);

    // Push job ID back to queue
    redis::pipe()
        .cmd("LPUSH").arg(&queue_key).arg(&job_id)
        .ignore()
        .cmd("HSET").arg(&job_key).arg("status").arg("queued")
        .ignore()
        .cmd("HSET").arg(&job_key).arg("queued_at").arg(Utc::now().to_rfc3339())
        .query_async::<_, ()>(&mut conn)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().json(json!({ "status": "ok", "message": "Job retried" })))
}







#[derive(Serialize)]
struct QueueSummary {
    queued: usize,
    failed: usize,
    success: usize,
}

pub async fn get_metrics_summary() -> impl Responder {
    let mut conn = match get_redis_conn().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let mut summary: HashMap<String, QueueSummary> = HashMap::new();

    let queues: Vec<String> = conn
    .keys::<_, Vec<String>>("snm:queue:*").await.unwrap_or_default()
    .iter()
    .filter_map(|k| k.strip_prefix("snm:queue:").map(|s| s.to_string()))
    .collect();

    for queue in queues {
        let queue_key = format!("snm:queue:{}", queue);
        let job_ids: Vec<String> = conn.lrange(&queue_key, 0, -1).await.unwrap_or_default();

        let mut queued = 0;
        let mut success = 0;
        let mut failed = 0;

        for job_id in &job_ids {
            let job_key = format!("snm:job:{job_id}");
            let status: String = conn.hget(&job_key, "status").await.unwrap_or_default();
            match status.as_str() {
                "queued" => queued += 1,
                "success" => success += 1,
                "failed" => failed += 1,
                _ => {}
            }
        }

        summary.insert(queue.clone(), QueueSummary { queued, failed, success });
    }

    HttpResponse::Ok().json(summary)
}


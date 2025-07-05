use actix_web::{web, HttpResponse, Responder};
use tera::Context;
use redis::AsyncCommands;
use chrono::Utc;
use serde_json::json;

use crate::utils::rdconfig::get_redis_connection;
use crate::services::template_service::render_template;
use crate::utils::pagination::{Pagination, PaginationQuery};
use crate::utils::constants::{DEFAULT_PAGE, DEFAULT_LIMIT};
use crate::utils::jconfig::{deserialize_job, to_job_info, JobInfo, fetch_job_info};
use crate::utils::renderer::paginate_jobs;

pub async fn render_metrics() -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let queues: Vec<String> = conn.smembers("snm:queues").await.unwrap_or_default();

    let mut ctx = Context::new();
    ctx.insert("title", "All Queues");
    ctx.insert("queues", &queues);
    render_template("metrics.html.tera", ctx).await
}

pub async fn render_metrics_for_queue(
    path: web::Path<String>,
    query: web::Query<PaginationQuery>,
) -> impl Responder {
    let queue = path.into_inner();

    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let key = format!("snm:queue:{}", queue);
    let all_jobs: Vec<String> = conn.lrange(&key, 0, -1).await.unwrap_or_default();

    let page = query.page.unwrap_or(DEFAULT_PAGE);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let (job_ids, pagination) = paginate_jobs(all_jobs, page, limit).await;

    let mut job_infos: Vec<JobInfo> = Vec::new();

    for job_id in job_ids {
        match fetch_job_info(&job_id).await {
            Ok(Some(info)) => job_infos.push(info),
            Ok(None) => {
                tracing::warn!("Job info not found for ID: {}", job_id);
            }
            Err(e) => {
                tracing::error!("Failed to fetch job info for ID {}: {:?}", job_id, e);
            }
        }
    }

    let mut ctx = Context::new();
    ctx.insert("title", &format!("Queue: {}", queue));
    ctx.insert("queue", &queue);
    ctx.insert("jobs", &job_infos); // JobInfo is Serialize
    ctx.insert("pagination", &pagination);

    render_template("queue_metrics.html.tera", ctx).await
}





pub async fn render_dead_jobs(query: web::Query<PaginationQuery>) -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let jobs: Vec<String> = conn.lrange("snm:dead_jobs", 0, -1).await.unwrap_or_default();

    let page = query.page.unwrap_or(DEFAULT_PAGE);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let (paginated_jobs, pagination) = paginate_jobs(jobs, page, limit).await;

    let mut ctx = Context::new();
    ctx.insert("title", "Dead Jobs");
    ctx.insert("jobs", &paginated_jobs);
    ctx.insert("pagination", &pagination);
    render_template("dead_jobs.html.tera", ctx).await
}




pub async fn render_scheduled_jobs(query: web::Query<PaginationQuery>) -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let now = Utc::now().timestamp();
    let job_ids: Vec<String> = conn
        .zrangebyscore("snm:delayed", 0, now)
        .await
        .unwrap_or_default();

    let mut job_infos = Vec::new();

    for jid in job_ids {
        if let Ok(data) = conn.get::<_, String>(format!("snm:job:{}", jid)).await {
            if let Some(job) = deserialize_job(data).await {
                job_infos.push(to_job_info(&job, &jid)); // ✅ Fixed: provide both arguments
            }
        }
    }

    let page = query.page.unwrap_or(DEFAULT_PAGE);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let total = job_infos.len();
    let pagination = Pagination::new(page, limit, total);

    let start = pagination.offset();
    let end = (start + limit).min(total);
    let paginated_job_infos = &job_infos[start..end];

    let mut ctx = Context::new();
    ctx.insert("title", "Scheduled Jobs");
    ctx.insert("jobs", &paginated_job_infos); // ✅ Safe for Tera (JobInfo implements Serialize)
    ctx.insert("pagination", &pagination);

    render_template("scheduled_jobs.html.tera", ctx).await
}




pub async fn render_worker_status() -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let keys: Vec<String> = conn.keys("snm:worker:*").await.unwrap_or_default();
    let mut workers = Vec::new();

    for key in keys {
        if let Ok(status) = conn.get::<_, String>(&key).await {
            workers.push((key, status));
        }
    }

    let mut ctx = Context::new();
    ctx.insert("title", "Worker Status");
    ctx.insert("workers", &workers);
    render_template("workers.html.tera", ctx).await
}

pub async fn job_action(payload: web::Json<serde_json::Value>) -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let action = payload.get("action").and_then(|a| a.as_str()).unwrap_or("");
    let job_id = payload.get("job_id").and_then(|j| j.as_str()).unwrap_or("");

    match action {
        "delete" => {
            let _: () = conn.del(format!("snm:job:{}", job_id)).await.unwrap_or_default();
            HttpResponse::Ok().json(json!({"status": "deleted"}))
        }
        _ => HttpResponse::BadRequest().json(json!({"error": "invalid action"})),
    }
}

pub async fn retry_job(payload: web::Json<serde_json::Value>) -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let job_id = payload.get("job_id").and_then(|j| j.as_str()).unwrap_or("");
    if let Ok(data) = conn.get::<_, String>(format!("snm:job:{}", job_id)).await {
        let _: () = conn.rpush("snm:queue:default", data).await.unwrap_or_default();
        HttpResponse::Ok().json(json!({"status": "retried"}))
    } else {
        HttpResponse::NotFound().json(json!({"error": "job not found"}))
    }
}

pub async fn get_metrics_summary() -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };
    let queues: Vec<String> = conn.smembers("snm:queues").await.unwrap_or_default();
    let mut summary = Vec::new();

    for queue in queues {
        let len: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
        summary.push((queue, len));
    }

    let mut ctx = Context::new();
    ctx.insert("title", "Metrics Summary");
    ctx.insert("summary", &summary);
    render_template("summary.html.tera", ctx).await
}


pub async fn export_queue_csv(path: web::Path<String>) -> impl Responder {
    let queue = path.into_inner();
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let key = format!("snm:queue:{}", queue);
    let jobs: Vec<String> = conn.lrange(&key, 0, -1).await.unwrap_or_default();

    let mut job_infos: Vec<JobInfo> = vec![];

    for (i, payload) in jobs.into_iter().enumerate() {
        if let Some(job) = deserialize_job(payload).await {
            let id = format!("{}_{}", queue, i); // Fallback ID for CSV export
            job_infos.push(to_job_info(&job, &id));
        }
    }

    let mut wtr = csv::Writer::from_writer(vec![]);
    for job_info in &job_infos {
        let _ = wtr.serialize(job_info);
    }

    let data = wtr.into_inner().unwrap_or_default();
    HttpResponse::Ok()
        .content_type("text/csv")
        .append_header(("Content-Disposition", format!("attachment; filename=queue_{}.csv", queue)))
        .body(data)
}


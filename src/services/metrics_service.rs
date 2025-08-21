use actix_web::{web, HttpResponse, Responder};
use tera::Context;
use redis::AsyncCommands;
use chrono::{Duration, Utc};
use serde_json::json;
use serde::{Deserialize, Serialize};
use csv::WriterBuilder;
use crate::utils::rdconfig::{get_redis_connection};
use crate::services::template_service::render_template;
use crate::utils::pagination::{Pagination, PaginationQuery};
use crate::utils::jconfig::{deserialize_job, to_job_info, JobInfo, fetch_job_info};
use crate::utils::renderer::paginate_jobs;
use crate::utils::constants::{
    DEFAULT_PAGE,
    DEFAULT_LIMIT,
    DELAYED_JOBS_KEY,
    PREFIX_QUEUE,
};

#[derive(Deserialize)]
pub struct MetricsQuery {
    pub search: Option<String>,
    pub page: Option<usize>,
    pub limit: Option<usize>,
}


pub async fn render_metrics(query: web::Query<MetricsQuery>) -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    // Fetch queues with jobs
    let active_queues: Vec<String> = conn.smembers("snm:queues").await.unwrap_or_default();
    
    // Get configured queues from Redis config keys
    let config_keys: Vec<String> = conn.keys("snm:queue:config:*").await.unwrap_or_default();
    let configured_queues: Vec<String> = config_keys
        .into_iter()
        .filter_map(|key| key.strip_prefix("snm:queue:config:").map(String::from))
        .collect();
    
    // Merge and deduplicate queues (configured queues take priority)
    let mut all_queues = configured_queues;
    for queue in active_queues {
        if !all_queues.contains(&queue) {
            all_queues.push(queue);
        }
    }
    
    // Apply search filter
    if let Some(search) = &query.search {
        let search_lower = search.to_lowercase();
        all_queues = all_queues
            .into_iter()
            .filter(|q| q.to_lowercase().contains(&search_lower))
            .collect();
    }

    // Convert query into Pagination
    let pagination_query = PaginationQuery {
        page: query.page,
        limit: query.limit,
    };
    let pagination = pagination_query.into_pagination(all_queues.len());

    // Paginate queues
    let (paginated_queues, pagination) =
        paginate_jobs(all_queues, pagination.page, pagination.limit).await;

    // Per-queue detailed stats
    let mut queue_infos = vec![];
    let mut total_success = 0;
    let mut total_failed = 0;
    let mut total_retry = 0;
    let mut total_pending = 0;

    for queue in &paginated_queues {
        let success_key = format!("snm:success:{}", queue);
        let failed_key = format!("snm:failed:{}", queue);
        let retry_key = format!("snm:retry:{}", queue);
        let pending_key = format!("snm:queue:{}", queue);

        let success: usize = conn.llen(&success_key).await.unwrap_or(0);
        let failed: usize = conn.llen(&failed_key).await.unwrap_or(0);
        let retry: usize = conn.llen(&retry_key).await.unwrap_or(0);
        let pending: usize = conn.llen(&pending_key).await.unwrap_or(0);

        total_success += success;
        total_failed += failed;
        total_retry += retry;
        total_pending += pending;

        queue_infos.push(json!({
            "name": queue,
            "success": success,
            "failed": failed,
            "retry": retry,
            "pending": pending
        }));
    }

    // Prepare context
    let mut ctx = Context::new();
    ctx.insert("title", "All Queues");
    ctx.insert("queues", &queue_infos);

    ctx.insert("stats", &json!({
        "success_jobs": total_success,
        "failed_jobs": total_failed,
        "retry_jobs": total_retry,
        "pending_jobs": total_pending,
    }));

    // Fix query object structure for template
    ctx.insert("query", &json!({
        "search": query.search.clone().unwrap_or_default()
    }));

    ctx.insert("page", &json!({
        "current": pagination.page,
        "start": pagination.offset() + 1,
        "end": (pagination.offset() + pagination.limit).min(pagination.total),
        "total": pagination.total,
        "has_prev": pagination.has_prev,
        "has_next": pagination.has_next,
        "query": format!("search={}", query.search.clone().unwrap_or_default())
    }));

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

    let all_jobs: Vec<String> = match conn.lrange(&key, 0, -1).await {
        Ok(jobs) => jobs,
        Err(_) => vec![], // fallback to empty if queue not found
    };

    let page = query.page.unwrap_or(DEFAULT_PAGE);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let (job_ids, pagination) = paginate_jobs(all_jobs, page, limit).await;

    // 🟫 5. Collect job info
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
    ctx.insert("jobs", &job_infos);
    ctx.insert("page", &serde_json::json!({
        "current": pagination.page,
        "start": pagination.offset() + 1,
        "end": (pagination.offset() + pagination.limit).min(pagination.total),
        "total": pagination.total,
        "has_prev": pagination.has_prev,
        "has_next": pagination.has_next,
        "query": "" // Add filters if any, like queue name or status
    }));

    render_template("queue_metrics.html.tera", ctx).await
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




pub async fn render_delayed_jobs(query: web::Query<PaginationQuery>) -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis connection failed"),
    };

    // 1. Get all delayed job IDs
    let job_ids: Vec<String> = match conn.zrange(DELAYED_JOBS_KEY, 0, -1).await {
        Ok(ids) => ids,
        Err(_) => vec![],
    };

    // 2. Fetch job info for each valid ID
    let mut job_infos = Vec::new();
    for jid in job_ids {
        match fetch_job_info(&jid).await {
            Ok(Some(info)) => job_infos.push(info),
            Ok(None) => {
                tracing::warn!("❌ Delayed job not found in Redis for ID: {}", jid);
            }
            Err(err) => {
                tracing::error!("❌ Failed to fetch delayed job ID {}: {:?}", jid, err);
            }
        }
    }

    // 3. Pagination logic
    let page = query.page.unwrap_or(DEFAULT_PAGE);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let total = job_infos.len();
    let pagination = Pagination::new(page, limit, total);

    let start = pagination.offset();
    let end = (start + limit).min(total);
    let paginated = &job_infos[start..end];

    // 4. Render Tera template
    let mut ctx = Context::new();
    ctx.insert("title", "All Delayed Jobs");
    ctx.insert("jobs", &paginated);
    ctx.insert("page", &serde_json::json!({
        "current": pagination.page,
        "start": start + 1,
        "end": end,
        "total": total,
        "has_prev": pagination.has_prev,
        "has_next": pagination.has_next,
        "query": ""
    }));

    render_template("delayed_jobs.html.tera", ctx).await
}



pub async fn render_dead_jobs(query: web::Query<PaginationQuery>) -> impl Responder {
    // 1. Connect to Redis
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Redis connection failed: {:?}", e);
            return HttpResponse::InternalServerError().body("Redis connection error");
        }
    };

    // 2. Fetch all failed job IDs
    let all_jobs: Vec<String> = match conn.lrange("snm:failed_jobs", 0, -1).await {
        Ok(jobs) => jobs,
        Err(e) => {
            tracing::error!("Failed to read failed_jobs list: {:?}", e);
            vec![]
        }
    };

    // 3. Apply pagination
    let page = query.page.unwrap_or(DEFAULT_PAGE);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    let (job_ids, pagination) = paginate_jobs(all_jobs, page, limit).await;

    // 4. Fetch job details
    let mut job_infos: Vec<JobInfo> = Vec::new();
    for job_id in job_ids {
        match fetch_job_info(&job_id).await {
            Ok(Some(info)) => job_infos.push(info),
            Ok(None) => {
                tracing::warn!("Job info not found for ID: {}", job_id);
            }
            Err(e) => {
                tracing::error!("Error fetching job info for ID {}: {:?}", job_id, e);
            }
        }
    }

    // 5. Render using Tera template
    let mut ctx = Context::new();
    ctx.insert("title", "Dead Jobs");
    ctx.insert("jobs", &job_infos);
    ctx.insert("page", &json!({
        "current": pagination.page,
        "start": pagination.offset() + 1,
        "end": (pagination.offset() + pagination.limit).min(pagination.total),
        "total": pagination.total,
        "has_prev": pagination.has_prev,
        "has_next": pagination.has_next,
        "query": ""
    }));

    render_template("dead_jobs.html.tera", ctx).await
}



#[derive(Debug, Serialize, Deserialize)]
struct WorkerStatus {
    id: String,
    queues: Vec<String>,
    last_seen: String,
    hostname: String,
    pid: u32,
}

pub async fn render_worker_status() -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let keys: Vec<String> = conn.keys("snm:worker:*").await.unwrap_or_default();
    let mut workers = Vec::new();

    for key in keys {
        if let Ok(status_json) = conn.get::<_, String>(&key).await {
            if let Ok(mut status) = serde_json::from_str::<WorkerStatus>(&status_json) {
                status.id = key.replace("snm:worker:", "");
                workers.push(status);
            }
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
    let job_key = format!("snm:job:{}", job_id);

    match action {
        "delete" => {
            if conn.exists(&job_key).await.unwrap_or(false) {
                let _: () = conn.del(&job_key).await.unwrap_or_default();
                let _: () = conn.lpush("snm:logs:default", format!("[{}] 🗑️ Job {} deleted", Utc::now(), job_id)).await.unwrap_or_default();
                let _: () = conn.ltrim("snm:logs:default", 0, 99).await.unwrap_or_default();
                HttpResponse::Ok().json(json!({"status": "deleted"}))
            } else {
                HttpResponse::NotFound().json(json!({"error": "job not found"}))
            }
        },
        "retry" | "queue" => {
            let exists: bool = conn.exists(&job_key).await.unwrap_or(false);
            if !exists {
                return HttpResponse::NotFound().json(json!({ "error": "job not found" }));
            }

            // Fetch queue name from the job
            let queue: String = conn.hget(&job_key, "queue").await.unwrap_or_else(|_| "default".to_string());
            let queue_key = format!("{PREFIX_QUEUE}:{}", queue);

            // Always remove job from delayed_jobs if present
            let _: () = conn.zrem(DELAYED_JOBS_KEY, &job_id).await.unwrap_or_default();

            // Also remove it from the Redis queue (avoid duplicates)
            let _: () = conn.lrem(&queue_key, 0, &job_id).await.unwrap_or_default();

            // Push job back to the correct queue
            let _: () = conn.rpush(&queue_key, &job_id).await.unwrap_or_default();

            // Update job metadata
            let _: () = conn.hset_multiple(&job_key, &[
                ("status", "queued"),
                ("retry_at", &Utc::now().to_rfc3339()),
            ]).await.unwrap_or_default();

            // Remove any stale timestamps
            let _: () = conn.hdel(&job_key, &["failed_at", "completed_at", "run_at"]).await.unwrap_or_default();

            HttpResponse::Ok().json(json!({ "status": "retried", "queue": queue }))
        }

        _ => HttpResponse::BadRequest().json(json!({"error": "invalid action"})),
    }
}



pub async fn get_metrics_summary() -> impl Responder {
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
    };

    let total_jobs: usize = conn.get("snm:qrush:total_jobs").await.unwrap_or(0);
    let success_jobs: usize = conn.get("snm:qrush:success").await.unwrap_or(0);
    let failed_jobs: usize = conn.get("snm:qrush:failed").await.unwrap_or(0);

    let queues: Vec<String> = conn.smembers("snm:queues").await.unwrap_or_default();
    let mut scheduled_jobs = 0;
    for queue in &queues {
        let len: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
        scheduled_jobs += len;
    }

    let worker_keys: Vec<String> = conn.keys("snm:worker:*").await.unwrap_or_default();
    let active_workers = worker_keys.len();

    // Collect last 7 days stats
    let mut chart_labels = Vec::new();
    let mut chart_success = Vec::new();
    let mut chart_failed = Vec::new();

    for i in (0..7).rev() {
        let day = Utc::now().date_naive() - Duration::days(i);
        let date_str = day.format("%Y-%m-%d").to_string();

        let total_key = format!("snm:stats:jobs:{}", date_str);
        let failed_key = format!("snm:stats:jobs:{}:failed", date_str);

        let total: usize = conn.get(&total_key).await.unwrap_or(0);
        let failed: usize = conn.get(&failed_key).await.unwrap_or(0);
        let success = total.saturating_sub(failed);

        chart_labels.push(day.format("%a").to_string()); // "Mon", "Tue", etc.
        chart_success.push(success);
        chart_failed.push(failed);
    }

    let mut ctx = Context::new();
    ctx.insert("title", "Metrics Summary");
    ctx.insert("stats", &json!({
        "total_jobs": total_jobs,
        "success_jobs": success_jobs,
        "failed_jobs": failed_jobs,
        "scheduled_jobs": scheduled_jobs,
        "active_workers": active_workers
    }));
    ctx.insert("chart", &json!({
        "labels": chart_labels,
        "success": chart_success,
        "failed": chart_failed
    }));

    render_template("summary.html.tera", ctx).await
}


// pub async fn get_metrics_summary() -> impl Responder {
//     let mut conn = match get_redis_connection().await {
//         Ok(c) => c,
//         Err(_) => return HttpResponse::InternalServerError().body("Redis error"),
//     };

//     let total_jobs: usize = conn.get("snm:qrush:total_jobs").await.unwrap_or(0);
//     let failed_jobs: usize = conn.get("snm:qrush:failed").await.unwrap_or(0);

//     let queues: Vec<String> = conn.smembers("snm:queues").await.unwrap_or_default();
//     let mut scheduled_jobs = 0;
//     for queue in &queues {
//         let len: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
//         scheduled_jobs += len;
//     }

//     let worker_keys: Vec<String> = conn.keys("snm:worker:*").await.unwrap_or_default();
//     let active_workers = worker_keys.len();

//     // REAL chart data from last 7 days
//     let mut chart_labels = Vec::new();
//     let mut chart_data = Vec::new();

//     for i in (0..7).rev() {
//         let day = Utc::now().date_naive() - Duration::days(i);
//         let date_str = day.format("%Y-%m-%d").to_string();
//         let redis_key = format!("snm:stats:jobs:{}", date_str);

//         let count: usize = conn.get(&redis_key).await.unwrap_or(0);
//         chart_labels.push(day.format("%a").to_string()); // "Mon", "Tue", etc.
//         chart_data.push(count);
//     }

//     let mut ctx = Context::new();
//     ctx.insert("title", "Metrics Summary");
//     ctx.insert("stats", &json!({
//         "total_jobs": total_jobs,
//         "failed_jobs": failed_jobs,
//         "scheduled_jobs": scheduled_jobs,
//         "active_workers": active_workers
//     }));
//     ctx.insert("chart", &json!({
//         "labels": chart_labels,
//         "data": chart_data
//     }));

//     render_template("summary.html.tera", ctx).await
// }



pub async fn export_queue_csv(path: web::Path<String>) -> impl Responder {
    let queue = path.into_inner();
    let mut conn = match get_redis_connection().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Redis connection error: {:?}", e);
            return HttpResponse::InternalServerError().body("Failed to connect to Redis");
        }
    };

    let key = format!("snm:queue:{}", queue);
    let jobs: Vec<String> = match conn.lrange(&key, 0, -1).await {
        Ok(j) => j,
        Err(e) => {
            eprintln!("Failed to fetch jobs from Redis: {:?}", e);
            return HttpResponse::InternalServerError().body("Failed to fetch jobs");
        }
    };

    let mut job_infos: Vec<JobInfo> = vec![];

    for (i, payload) in jobs.into_iter().enumerate() {
        match deserialize_job(payload).await {
            Some(job) => {
                let id = format!("{}_{}", queue, i); // fallback ID
                job_infos.push(to_job_info(&job, &id));
            }
            None => {
                eprintln!("Failed to deserialize job at index {}", i);
                continue;
            }
        }
    }

    let mut wtr = WriterBuilder::new()
        .has_headers(true)
        .from_writer(vec![]);

    for job_info in &job_infos {
        if let Err(e) = wtr.serialize(job_info) {
            eprintln!("CSV serialization failed: {:?}", e);
        }
    }

    let data = match wtr.into_inner() {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Failed to build CSV output: {:?}", e);
            return HttpResponse::InternalServerError().body("Failed to generate CSV");
        }
    };

    HttpResponse::Ok()
        .content_type("text/csv")
        .append_header(("Content-Disposition", format!("attachment; filename=queue_{}.csv", queue)))
        .body(data)
}


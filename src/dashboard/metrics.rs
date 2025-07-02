
use crate::config::get_global_queues;
use crate::redis_pool::get_redis_conn;
use redis::AsyncCommands;
use std::collections::HashMap;
use tera::{Context, Tera};
use anyhow::{Result, Context as AnyhowContext};
use lazy_static::lazy_static;
use serde::Deserialize;
use serde_json::{json, Value};

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = Tera::default();
        
        // Add debug logging
        let template_content = include_str!("../templates/metrics.html.tera");
        println!("Template content loaded, length: {}", template_content.len());
        
        match tera.add_raw_template("metrics.html.tera", template_content) {
            Ok(_) => println!("Template added successfully"),
            Err(e) => panic!("Failed to add template: {}", e),
        }
        
        tera
    };
}

const PAGE_SIZE: usize = 10;

#[derive(Deserialize)]
struct DelayedJobMeta {
    queue: String,
}

pub async fn render_metrics(page: usize) -> Result<(String, usize, bool)> {
    let mut conn = get_redis_conn().await
        .with_context(|| "Failed to get Redis connection")?;
    
    let all_queues = get_global_queues();
    let start = (page - 1) * PAGE_SIZE;
    let end = start + PAGE_SIZE;

    let paginated_queues = all_queues.iter().skip(start).take(PAGE_SIZE);
    let mut queues: Vec<HashMap<String, Value>> = vec![];

    // Read all delayed jobs once
    let delayed_key = "snm:delayed_jobs";
    let all_delayed: Vec<String> = conn.zrange(delayed_key, 0, -1).await.unwrap_or_default();

    let mut total_failed_jobs = 0;
    let mut total_success_jobs = 0;

    for config in paginated_queues {
        let queue_key = format!("snm:queue:{}", config.name);
        let success_key = format!("snm:success:{}", config.name);
        let failed_key = format!("snm:failed:{}", config.name);

        let pending_jobs: isize = conn.llen(&queue_key).await.unwrap_or(0);
        let success_jobs: isize = conn.get(&success_key).await.unwrap_or(0);
        let failed_jobs: isize = conn.get(&failed_key).await.unwrap_or(0);

        // Filter delayed jobs for this queue
        let mut filtered_delayed: Vec<String> = vec![];
        for job_id in &all_delayed {
            let job_key = format!("snm:job:{job_id}");
            let meta: HashMap<String, String> = conn.hgetall(&job_key).await.unwrap_or_default();
            if let Some(queue) = meta.get("queue") {
                if queue == &config.name {
                    filtered_delayed.push(job_id.clone());
                }
            }
        }

        let delayed_jobs = filtered_delayed.len() as isize;
        let total_jobs = pending_jobs + delayed_jobs;

        // Get sample jobs
        let top_pending: Vec<String> = conn.lrange(&queue_key, 0, 1).await.unwrap_or_default();
        let top_delayed: Vec<String> = filtered_delayed.into_iter().take(2).collect();

        let mut jobs = vec![];

        // Add pending jobs
        for job in top_pending {
            let parsed: Value = serde_json::from_str(&job).unwrap_or_else(|_| json!({}));
            jobs.push(json!({
                "id": job,
                "status": "pending",
                "payload": parsed
            }));
        }

        // Add delayed jobs
        for job in top_delayed {
            let job_key = format!("snm:job:{job}");
            let payload: String = conn.hget(&job_key, "payload").await.unwrap_or_default();
            let parsed: Value = serde_json::from_str(&payload).unwrap_or_else(|_| json!({}));
            jobs.push(json!({
                "id": job,
                "status": "delayed",
                "payload": parsed
            }));
        }

        let mut queue_info = HashMap::new();
        queue_info.insert("name".to_string(), json!(config.name));
        queue_info.insert("concurrency".to_string(), json!(config.concurrency));
        queue_info.insert("priority".to_string(), json!(config.priority));
        queue_info.insert("pending_jobs".to_string(), json!(pending_jobs));
        queue_info.insert("delayed_jobs".to_string(), json!(delayed_jobs));
        queue_info.insert("success_jobs".to_string(), json!(success_jobs));
        queue_info.insert("failed_jobs".to_string(), json!(failed_jobs));
        queue_info.insert("total_jobs".to_string(), json!(total_jobs));
        queue_info.insert("jobs".to_string(), json!(jobs));

        queues.push(queue_info);

        total_failed_jobs += failed_jobs;
        total_success_jobs += success_jobs;
    }

    let mut context = Context::new();
    context.insert("queues", &queues);
    context.insert("page", &page);
    context.insert("has_more", &(all_queues.len() > end));

    let mut stats = HashMap::new();
    stats.insert("success", Value::from(total_success_jobs));
    stats.insert("failed", Value::from(total_failed_jobs));
    context.insert("stats", &stats);

    // Debug output
    println!("Rendering template with {} queues, page {}", queues.len(), page);
    println!("Stats: success={}, failed={}", total_success_jobs, total_failed_jobs);

    let html = TEMPLATES.render("metrics.html.tera", &context)
        .with_context(|| "Failed to render template")?;

    println!("Successfully rendered HTML, length: {}", html.len());

    Ok((html, page, all_queues.len() > end))
}
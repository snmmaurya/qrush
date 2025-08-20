// crates/qrush/src/services/cron_service.rs

use actix_web::{web, HttpResponse, Responder};
use serde::Deserialize;
use tera::Context;
use crate::cron::cron_scheduler::CronScheduler;
use crate::services::template_service::render_template;

#[derive(Deserialize)]
pub struct CronActionRequest {
    pub action: String,
    pub job_id: String,
    pub enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct CreateCronJobRequest {
    pub name: String,
    pub queue: String,
    pub cron_expression: String,
    pub job_type: String,
    pub payload: serde_json::Value,
}

pub async fn render_cron_jobs() -> impl Responder {
    match CronScheduler::list_cron_jobs().await {
        Ok(cron_jobs) => {
            let mut ctx = Context::new();
            ctx.insert("title", "Cron Jobs");
            ctx.insert("cron_jobs", &cron_jobs);
            render_template("cron_jobs.html.tera", ctx).await
        }
        Err(e) => {
            eprintln!("Failed to fetch cron jobs: {:?}", e);
            HttpResponse::InternalServerError().body("Failed to fetch cron jobs")
        }
    }
}

pub async fn cron_action(payload: web::Json<CronActionRequest>) -> impl Responder {
    let action = &payload.action;
    let job_id = &payload.job_id;

    let result = match action.as_str() {
        "toggle" => {
            let enabled = payload.enabled.unwrap_or(true);
            CronScheduler::toggle_cron_job(job_id, enabled).await
        }
        "delete" => {
            CronScheduler::delete_cron_job(job_id).await
        }
        "run_now" => {
            match CronScheduler::run_now(job_id).await {
                Ok(enqueued_id) => {
                    return HttpResponse::Ok().json(serde_json::json!({
                        "status": "success",
                        "action": "run_now",
                        "enqueued_job_id": enqueued_id
                    }));
                }
                Err(e) => Err(e)
            }
        }
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "error": "Invalid action"
            }));
        }
    };

    match result {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({
            "status": "success",
            "action": action
        })),
        Err(e) => HttpResponse::InternalServerError().json(serde_json::json!({
            "error": e.to_string()
        }))
    }
}

pub async fn create_cron_job(_payload: web::Json<CreateCronJobRequest>) -> impl Responder {
    // This would need to be implemented based on your job registry
    // For now, return a placeholder response
    HttpResponse::Ok().json(serde_json::json!({
        "status": "Cron job creation endpoint - implement based on your job types"
    }))
}
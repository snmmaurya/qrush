use actix_web::HttpResponse;
use once_cell::sync::Lazy;
use std::sync::Arc;
use tera::{Context, Tera};

// Centralized template list to keep code clean and DRY
const TEMPLATE_FILES: &[(&str, &str)] = &[
    ("layout.html.tera", include_str!("../templates/layout.html.tera")),
    ("dead_jobs.html.tera", include_str!("../templates/dead_jobs.html.tera")),
    ("metrics.html.tera", include_str!("../templates/metrics.html.tera")),
    ("queue_metrics.html.tera", include_str!("../templates/queue_metrics.html.tera")),
    ("scheduled_jobs.html.tera", include_str!("../templates/scheduled_jobs.html.tera")),
    ("delayed_jobs.html.tera", include_str!("../templates/delayed_jobs.html.tera")),
    ("summary.html.tera", include_str!("../templates/summary.html.tera")),
    ("workers.html.tera", include_str!("../templates/workers.html.tera")),
    ("cron_jobs.html.tera", include_str!("../templates/cron_jobs.html.tera")),
    ("errors/404.html.tera", include_str!("../templates/errors/404.html.tera")),
    ("errors/500.html.tera", include_str!("../templates/errors/500.html.tera")),
];

static TEMPLATES: Lazy<Arc<Tera>> = Lazy::new(|| {
    let mut tera = Tera::default();

    for (name, content) in TEMPLATE_FILES {
        tera.add_raw_template(name, content)
            .unwrap_or_else(|e| panic!("Failed to add {}: {}", name, e));
    }

    tera.autoescape_on(vec![]); // Disable autoescaping if rendering raw HTML
    Arc::new(tera)
});

pub async fn render_template(template_name: &str, ctx: Context) -> HttpResponse {
    let tera = Arc::clone(&TEMPLATES);
    match tera.render(template_name, &ctx) {
        Ok(html) => HttpResponse::Ok().content_type("text/html").body(html),
        Err(err) => {
            eprintln!("Template render error: {:?}", err);
            let mut ctx = Context::new();
            ctx.insert("error", &err.to_string());
            let fallback_html = tera
                .render("errors/500.html.tera", &ctx)
                .unwrap_or_else(|_| "Internal Server Error".to_string());
            HttpResponse::InternalServerError().body(fallback_html)
        }
    }
}

// Optional helper for 404 rendering
pub async fn render_404() -> HttpResponse {
    let tera = Arc::clone(&TEMPLATES);
    let ctx = Context::new();
    let html = tera
        .render("errors/404.html.tera", &ctx)
        .unwrap_or_else(|_| "Page Not Found".to_string());
    HttpResponse::NotFound().body(html)
}

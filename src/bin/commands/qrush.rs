// src/bin/commands/qrush.rs
use clap::ArgMatches;
use redis::AsyncCommands;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process;
use std::time::Duration;
use tokio::time::{sleep, interval};
use tokio::signal;
use chrono::{Utc};
use anyhow::Result;
use colored::*;

use crate::get_redis_connection;

// Start QRush workers
pub async fn start_command(matches: &ArgMatches) -> Result<()> {
    let queues: Vec<&str> = matches.get_one::<String>("queues")
        .unwrap()
        .split(',')
        .collect();
    let concurrency: usize = matches.get_one::<String>("concurrency")
        .unwrap()
        .parse()
        .unwrap_or(5);
    let environment = matches.get_one::<String>("environment").unwrap();
    let daemon = matches.get_flag("daemon");
    
    println!("{}", "🚀 Starting QRush workers...".green().bold());
    println!("Queues: {}", queues.join(", "));
    println!("Concurrency: {}", concurrency);
    println!("Environment: {}", environment);
    
    if daemon {
        println!("Running as daemon...");
        // TODO: Implement proper daemonization
    }
    
    // Store worker info in Redis
    let mut conn = get_redis_connection().await?;
    let worker_id = format!("worker_{}_{}", gethostname::gethostname().to_string_lossy(), process::id());
    let worker_key = format!("snm:worker:{}", worker_id);
    
    let worker_info = json!({
        "id": worker_id,
        "hostname": gethostname::gethostname().to_string_lossy(),
        "pid": process::id(),
        "queues": queues,
        "concurrency": concurrency,
        "started_at": Utc::now().to_rfc3339(),
        "last_seen": Utc::now().to_rfc3339()
    });
    
    let _: () = conn.set(&worker_key, worker_info.to_string()).await?;
    let _: () = conn.expire(&worker_key, 300).await?; // Expire after 5 minutes
    
    // Set up signal handling
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
    };
    
    // Update heartbeat every 30 seconds
    let heartbeat = async {
        let mut interval = interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            if let Ok(mut conn) = get_redis_connection().await {
                let update = json!({
                    "last_seen": Utc::now().to_rfc3339()
                });
                let _: Result<(), _> = conn.hset(&worker_key, "last_seen", update["last_seen"].as_str().unwrap()).await;
                let _: Result<(), _> = conn.expire(&worker_key, 300).await;
            }
        }
    };
    
    println!("{}", "✅ Workers started. Press Ctrl+C to stop.".green());
    
    tokio::select! {
        _ = ctrl_c => {
            println!("\n{}", "🛑 Shutting down workers...".yellow());
            // Cleanup worker from Redis
            let _: Result<(), _> = conn.del(&worker_key).await;
            println!("{}", "✅ Workers stopped.".green());
        }
        _ = heartbeat => {}
    }
    
    Ok(())
}

// Stop QRush workers
pub async fn stop_command(matches: &ArgMatches) -> Result<()> {
    let timeout: u64 = matches.get_one::<String>("timeout")
        .unwrap()
        .parse()
        .unwrap_or(25);
    
    println!("{}", "🛑 Stopping QRush workers...".yellow());
    
    let mut conn = get_redis_connection().await?;
    
    // Get all worker keys
    let worker_keys: Vec<String> = conn.keys("snm:worker:*").await?;
    
    if worker_keys.is_empty() {
        println!("{}", "ℹ️  No running workers found.".blue());
        return Ok(());
    }
    
    // Send shutdown signal to all workers
    for key in &worker_keys {
        let _: () = conn.set(format!("{}:shutdown", key), "1").await?;
        let _: () = conn.expire(format!("{}:shutdown", key), timeout as i64).await?;
    }
    
    println!("Sent shutdown signal to {} workers", worker_keys.len());
    println!("Waiting up to {} seconds for graceful shutdown...", timeout);
    
    // Wait for workers to stop
    for i in 0..timeout {
        let remaining_workers: Vec<String> = conn.keys("snm:worker:*").await?;
        if remaining_workers.is_empty() {
            println!("{}", "✅ All workers stopped.".green());
            return Ok(());
        }
        
        if i % 5 == 0 {
            println!("Still waiting for {} workers...", remaining_workers.len());
        }
        
        sleep(Duration::from_secs(1)).await;
    }
    
    // Force cleanup if timeout reached
    let _: () = conn.del(&worker_keys).await?;
    println!("{}", "⚠️  Forced cleanup after timeout.".yellow());
    
    Ok(())
}

// Restart workers
pub async fn restart_command(matches: &ArgMatches) -> Result<()> {
    println!("{}", "🔄 Restarting QRush workers...".blue());
    
    stop_command(matches).await?;
    sleep(Duration::from_secs(2)).await;
    start_command(matches).await?;
    
    Ok(())
}

// Show status
pub async fn status_command(matches: &ArgMatches) -> Result<()> {
    let verbose = matches.get_flag("verbose");
    let mut conn = get_redis_connection().await?;
    
    println!("{}", "📊 QRush Status".blue().bold());
    println!("{}", "=".repeat(50).blue());
    
    // Get worker information
    let worker_keys: Vec<String> = conn.keys("snm:worker:*").await?;
    println!("Active Workers: {}", worker_keys.len().to_string().green());
    
    if verbose && !worker_keys.is_empty() {
        println!("\n{}", "Workers:".bold());
        for key in worker_keys {
            if let Ok(worker_data) = conn.get::<_, String>(&key).await {
                if let Ok(worker_info) = serde_json::from_str::<Value>(&worker_data) {
                    println!("  • {} (PID: {}, Host: {})", 
                        worker_info["id"].as_str().unwrap_or("unknown"),
                        worker_info["pid"].as_u64().unwrap_or(0),
                        worker_info["hostname"].as_str().unwrap_or("unknown")
                    );
                    if let Some(queues) = worker_info["queues"].as_array() {
                        let queue_names: Vec<String> = queues.iter()
                            .map(|q| q.as_str().unwrap_or("unknown").to_string())
                            .collect();
                        println!("    Queues: {}", queue_names.join(", "));
                    }
                }
            }
        }
    }
    
    // Get queue statistics
    let queues: Vec<String> = conn.smembers("snm:queues").await?;
    println!("\nTotal Queues: {}", queues.len().to_string().green());
    
    let mut total_pending = 0;
    let mut total_failed = 0;
    let mut total_success = 0;
    
    if verbose && !queues.is_empty() {
        println!("\n{}", "Queue Details:".bold());
        for queue in &queues {
            let pending: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
            let failed: usize = conn.llen(format!("snm:failed:{}", queue)).await.unwrap_or(0);
            let success: usize = conn.llen(format!("snm:success:{}", queue)).await.unwrap_or(0);
            
            total_pending += pending;
            total_failed += failed;
            total_success += success;
            
            println!("  • {}: {} pending, {} failed, {} success", 
                queue, 
                pending.to_string().yellow(),
                failed.to_string().red(),
                success.to_string().green()
            );
        }
    } else {
        for queue in &queues {
            let pending: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
            let failed: usize = conn.llen(format!("snm:failed:{}", queue)).await.unwrap_or(0);
            let success: usize = conn.llen(format!("snm:success:{}", queue)).await.unwrap_or(0);
            
            total_pending += pending;
            total_failed += failed;
            total_success += success;
        }
    }
    
    println!("\n{}", "Overall Statistics:".bold());
    println!("  Pending Jobs: {}", total_pending.to_string().yellow());
    println!("  Failed Jobs: {}", total_failed.to_string().red());
    println!("  Successful Jobs: {}", total_success.to_string().green());
    
    // Get delayed jobs count
    let delayed_count: usize = conn.zcard("snm:delayed_jobs").await.unwrap_or(0);
    println!("  Delayed Jobs: {}", delayed_count.to_string().blue());
    
    // Get cron jobs count
    let cron_keys: Vec<String> = conn.keys("snm:cron:jobs:*").await?;
    println!("  Cron Jobs: {}", cron_keys.len().to_string().cyan());
    
    Ok(())
}

// Show statistics with optional watching
pub async fn stats_command(matches: &ArgMatches) -> Result<()> {
    let queue = matches.get_one::<String>("queue");
    let watch = matches.get_flag("watch");
    
    if watch {
        println!("{}", "📈 Watching QRush Stats (Press Ctrl+C to stop)".blue().bold());
        let mut interval = interval(Duration::from_secs(2));
        
        loop {
            // Clear screen
            print!("\x1B[2J\x1B[1;1H");
            
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = show_stats(queue).await {
                        eprintln!("Error: {}", e);
                        break;
                    }
                }
                _ = signal::ctrl_c() => {
                    println!("\n{}", "👋 Stopped watching.".green());
                    break;
                }
            }
        }
    } else {
        show_stats(queue).await?;
    }
    
    Ok(())
}

async fn show_stats(queue_filter: Option<&String>) -> Result<()> {
    let mut conn = get_redis_connection().await?;
    
    println!("{} {}", "📊".blue(), format!("QRush Statistics - {}", Utc::now().format("%H:%M:%S")).blue().bold());
    println!("{}", "=".repeat(60).blue());
    
    let queues: Vec<String> = if let Some(q) = queue_filter {
        vec![q.clone()]
    } else {
        conn.smembers("snm:queues").await?
    };
    
    if queues.is_empty() {
        println!("{}", "No queues found.".yellow());
        return Ok(());
    }
    
    println!("{:<15} {:<8} {:<8} {:<8} {:<8}", 
        "Queue".bold(), 
        "Pending".yellow().bold(), 
        "Failed".red().bold(), 
        "Success".green().bold(),
        "Delayed".blue().bold()
    );
    println!("{}", "-".repeat(60));
    
    for queue in queues {
        let pending: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
        let failed: usize = conn.llen(format!("snm:failed:{}", queue)).await.unwrap_or(0);
        let success: usize = conn.llen(format!("snm:success:{}", queue)).await.unwrap_or(0);
        let delayed: usize = conn.zcard("snm:delayed_jobs").await.unwrap_or(0);
        
        println!("{:<15} {:<8} {:<8} {:<8} {:<8}", 
            queue, 
            pending.to_string().yellow(),
            failed.to_string().red(),
            success.to_string().green(),
            delayed.to_string().blue()
        );
    }
    
    Ok(())
}

// List queues
pub async fn queues_command(matches: &ArgMatches) -> Result<()> {
    let verbose = matches.get_flag("verbose");
    let mut conn = get_redis_connection().await?;
    
    let queues: Vec<String> = conn.smembers("snm:queues").await?;
    
    if queues.is_empty() {
        println!("{}", "No queues found.".yellow());
        return Ok(());
    }
    
    println!("{}", "📋 Available Queues".blue().bold());
    println!("{}", "=".repeat(30).blue());
    
    for queue in queues {
        if verbose {
            let pending: usize = conn.llen(format!("snm:queue:{}", queue)).await.unwrap_or(0);
            let failed: usize = conn.llen(format!("snm:failed:{}", queue)).await.unwrap_or(0);
            let success: usize = conn.llen(format!("snm:success:{}", queue)).await.unwrap_or(0);
            
            println!("• {} ({} pending, {} failed, {} success)", 
                queue.green(),
                pending.to_string().yellow(),
                failed.to_string().red(),
                success.to_string().green()
            );
        } else {
            println!("• {}", queue.green());
        }
    }
    
    Ok(())
}

// Handle job commands
pub async fn jobs_command(matches: &ArgMatches) -> Result<()> {
    match matches.subcommand() {
        Some(("list", sub_matches)) => list_jobs_command(sub_matches).await,
        Some(("retry", sub_matches)) => retry_jobs_command(sub_matches).await,
        Some(("delete", sub_matches)) => delete_jobs_command(sub_matches).await,
        Some(("enqueue", sub_matches)) => enqueue_job_command(sub_matches).await,
        _ => {
            println!("Invalid job command. Use 'qrush jobs --help' for usage.");
            Ok(())
        }
    }
}

async fn list_jobs_command(matches: &ArgMatches) -> Result<()> {
    let queue = matches.get_one::<String>("queue").unwrap();
    let status = matches.get_one::<String>("status").unwrap();
    let limit: usize = matches.get_one::<String>("limit").unwrap().parse().unwrap_or(10);
    
    let mut conn = get_redis_connection().await?;
    
    let key = match status.as_str() {
        "pending" => format!("snm:queue:{}", queue),
        "failed" => format!("snm:failed:{}", queue),
        "success" => format!("snm:success:{}", queue),
        "delayed" => "snm:delayed_jobs".to_string(),
        _ => {
            println!("Invalid status. Use: pending, failed, success, delayed");
            return Ok(());
        }
    };
    
    let job_ids: Vec<String> = if status == "delayed" {
        conn.zrange(&key, 0, (limit as isize) - 1).await?
    } else {
        conn.lrange(&key, 0, (limit as isize) - 1).await?
    };
    
    if job_ids.is_empty() {
        println!("No {} jobs found in queue '{}'.", status, queue);
        return Ok(());
    }
    
    println!("{}", format!("📋 {} Jobs in '{}' Queue (showing {})", 
        status.to_uppercase(), queue, job_ids.len()).blue().bold());
    println!("{}", "=".repeat(60).blue());
    
    for job_id in job_ids {
        let job_key = format!("snm:job:{}", job_id);
        if let Ok(job_data) = conn.hgetall::<_, HashMap<String, String>>(&job_key).await {
            println!("• {} {}", "ID:".bold(), job_id.green());
            
            if let Some(queue) = job_data.get("queue") {
                println!("  Queue: {}", queue);
            }
            if let Some(status) = job_data.get("status") {
                let colored_status = match status.as_str() {
                    "success" => status.green(),
                    "failed" => status.red(),
                    "pending" => status.yellow(),
                    _ => status.normal(),
                };
                println!("  Status: {}", colored_status);
            }
            if let Some(created_at) = job_data.get("created_at") {
                println!("  Created: {}", created_at);
            }
            if let Some(error) = job_data.get("error") {
                if !error.is_empty() {
                    println!("  Error: {}", error.red());
                }
            }
            println!();
        }
    }
    
    Ok(())
}

async fn retry_jobs_command(matches: &ArgMatches) -> Result<()> {
    let job_id = matches.get_one::<String>("job_id");
    let retry_all = matches.get_flag("all");
    
    let mut conn = get_redis_connection().await?;
    
    if retry_all {
        println!("{}", "🔄 Retrying all failed jobs...".blue());
        
        let failed_jobs: Vec<String> = conn.lrange("snm:failed_jobs", 0, -1).await?;
        let mut retried_count = 0;
        
        for job_id in failed_jobs {
            let job_key = format!("snm:job:{}", job_id);
            if let Ok(queue) = conn.hget::<_, _, String>(&job_key, "queue").await {
                let queue_key = format!("snm:queue:{}", queue);
                let _: () = conn.rpush(&queue_key, &job_id).await?;
                let _: () = conn.hset(&job_key, "status", "pending").await?;
                let _: () = conn.hdel(&job_key, &["failed_at", "error"]).await?;
                retried_count += 1;
            }
        }
        
        let _: () = conn.del("snm:failed_jobs").await?;
        println!("{}", format!("✅ Retried {} jobs.", retried_count).green());
        
    } else if let Some(job_id) = job_id {
        println!("{}", format!("🔄 Retrying job {}...", job_id).blue());
        
        let job_key = format!("snm:job:{}", job_id);
        if let Ok(queue) = conn.hget::<_, _, String>(&job_key, "queue").await {
            let queue_key = format!("snm:queue:{}", queue);
            let _: () = conn.rpush(&queue_key, job_id).await?;
            let _: () = conn.hset(&job_key, "status", "pending").await?;
            let _: () = conn.hdel(&job_key, &["failed_at", "error"]).await?;
            println!("{}", "✅ Job retried successfully.".green());
        } else {
            println!("{}", "❌ Job not found.".red());
        }
    } else {
        println!("Specify a job ID or use --all to retry all failed jobs.");
    }
    
    Ok(())
}

async fn delete_jobs_command(matches: &ArgMatches) -> Result<()> {
    let job_id = matches.get_one::<String>("job_id");
    let delete_failed = matches.get_flag("failed");
    
    let mut conn = get_redis_connection().await?;
    
    if delete_failed {
        println!("{}", "🗑️  Deleting all failed jobs...".yellow());
        
        let failed_jobs: Vec<String> = conn.lrange("snm:failed_jobs", 0, -1).await?;
        let mut deleted_count = 0;
        
        for job_id in failed_jobs {
            let job_key = format!("snm:job:{}", job_id);
            let _: () = conn.del(&job_key).await?;
            deleted_count += 1;
        }
        
        let _: () = conn.del("snm:failed_jobs").await?;
        println!("{}", format!("✅ Deleted {} failed jobs.", deleted_count).green());
        
    } else if let Some(job_id) = job_id {
        println!("{}", format!("🗑️  Deleting job {}...", job_id).yellow());
        
        let job_key = format!("snm:job:{}", job_id);
        let deleted: usize = conn.del(&job_key).await?;
        
        if deleted > 0 {
            println!("{}", "✅ Job deleted successfully.".green());
        } else {
            println!("{}", "❌ Job not found.".red());
        }
    } else {
        println!("Specify a job ID or use --failed to delete all failed jobs.");
    }
    
    Ok(())
}

async fn enqueue_job_command(matches: &ArgMatches) -> Result<()> {
    let queue = matches.get_one::<String>("queue").unwrap();
    let job_type = matches.get_one::<String>("job_type").unwrap();
    let payload = matches.get_one::<String>("payload").unwrap();
    let delay = matches.get_one::<String>("delay").map(|d| d.parse::<u64>().unwrap_or(0));
    
    println!("{}", format!("📤 Enqueueing {} job to '{}' queue...", job_type, queue).blue());
    
    // Create a test job payload
    let job_payload = json!({
        "job_type": job_type,
        "payload": serde_json::from_str::<Value>(payload)?,
        "queue": queue
    });
    
    let mut conn = get_redis_connection().await?;
    let job_id = format!("test_{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
    let job_key = format!("snm:job:{}", job_id);
    let now = Utc::now().to_rfc3339();
    
    if let Some(delay_secs) = delay {
        // Delayed job
        let run_at = Utc::now().timestamp() + delay_secs as i64;
        
        let _: () = conn.hset_multiple(&job_key, &[
            ("queue", queue.as_str()),
            ("status", "delayed"),
            ("payload", &job_payload.to_string()),
            ("created_at", &now),
            ("run_at", &run_at.to_string()),
        ]).await?;
        
        let _: () = conn.zadd("snm:delayed_jobs", &job_id, run_at).await?;
        println!("{}", format!("✅ Delayed job {} enqueued (delay: {}s).", job_id, delay_secs).green());
        
    } else {
        // Immediate job
        let queue_key = format!("snm:queue:{}", queue);
        
        let _: () = conn.hset_multiple(&job_key, &[
            ("queue", queue.as_str()),
            ("status", "pending"),
            ("payload", &job_payload.to_string()),
            ("created_at", &now),
        ]).await?;
        
        let _: () = conn.rpush(&queue_key, &job_id).await?;
        let _: () = conn.sadd("snm:queues", queue).await?;
        
        println!("{}", format!("✅ Job {} enqueued successfully.", job_id).green());
    }
    
    Ok(())
}

// Handle cron commands
pub async fn cron_command(matches: &ArgMatches) -> Result<()> {
    match matches.subcommand() {
        Some(("list", _)) => list_cron_jobs().await,
        Some(("enable", sub_matches)) => {
            let job_id = sub_matches.get_one::<String>("job_id").unwrap();
            toggle_cron_job(job_id, true).await
        },
        Some(("disable", sub_matches)) => {
            let job_id = sub_matches.get_one::<String>("job_id").unwrap();
            toggle_cron_job(job_id, false).await
        },
        Some(("run", sub_matches)) => {
            let job_id = sub_matches.get_one::<String>("job_id").unwrap();
            run_cron_job_now(job_id).await
        },
        _ => {
            println!("Invalid cron command. Use 'qrush cron --help' for usage.");
            Ok(())
        }
    }
}

async fn list_cron_jobs() -> Result<()> {
    let mut conn = get_redis_connection().await?;
    let cron_keys: Vec<String> = conn.keys("snm:cron:jobs:*").await?;
    
    if cron_keys.is_empty() {
        println!("{}", "No cron jobs found.".yellow());
        return Ok(());
    }
    
    println!("{}", "⏰ Cron Jobs".blue().bold());
    println!("{}", "=".repeat(80).blue());
    println!("{:<20} {:<15} {:<15} {:<15} {:<15}", 
        "ID".bold(), 
        "Name".bold(), 
        "Schedule".bold(), 
        "Status".bold(),
        "Next Run".bold()
    );
    println!("{}", "-".repeat(80));
    
    for key in cron_keys {
        if let Ok(job_data) = conn.hgetall::<_, HashMap<String, String>>(&key).await {
            let id = key.strip_prefix("snm:cron:jobs:").unwrap_or("unknown");
            let name = job_data.get("name").cloned().unwrap_or_else(|| "unknown".to_string());
            let cron_expression = job_data.get("cron_expression").cloned().unwrap_or_else(|| "unknown".to_string());
            let enabled = job_data.get("enabled").unwrap_or(&"false".to_string()) == "true";
            let next_run = job_data.get("next_run").cloned().unwrap_or_else(|| "unknown".to_string());
            
            let status = if enabled { "Enabled".green() } else { "Disabled".red() };
            
            println!("{:<20} {:<15} {:<15} {:<15} {:<15}", 
                id, 
                name, 
                cron_expression,
                status,
                next_run
            );
        }
    }
    
    Ok(())
}

async fn toggle_cron_job(job_id: &str, enabled: bool) -> Result<()> {
    let mut conn = get_redis_connection().await?;
    let job_key = format!("snm:cron:jobs:{}", job_id);
    
    let exists: bool = conn.exists(&job_key).await?;
    if !exists {
        println!("{}", "❌ Cron job not found.".red());
        return Ok(());
    }
    
    let _: () = conn.hset(&job_key, "enabled", enabled.to_string()).await?;
    
    let action = if enabled { "enabled" } else { "disabled" };
    println!("{}", format!("✅ Cron job {} {}.", job_id, action).green());
    
    Ok(())
}

async fn run_cron_job_now(job_id: &str) -> Result<()> {
    let mut conn = get_redis_connection().await?;
    let job_key = format!("snm:cron:jobs:{}", job_id);
    
    let exists: bool = conn.exists(&job_key).await?;
    if !exists {
        println!("{}", "❌ Cron job not found.".red());
        return Ok(());
    }
    
    if let Ok(job_data) = conn.hgetall::<_, HashMap<String, String>>(&job_key).await {
        if let (Some(payload), Some(queue)) = (job_data.get("payload"), job_data.get("queue")) {
            // Create immediate job from cron job
            let job_id = format!("cron_manual_{}", Utc::now().timestamp_nanos_opt().unwrap_or(0));
            let new_job_key = format!("snm:job:{}", job_id);
            let queue_key = format!("snm:queue:{}", queue);
            let now = Utc::now().to_rfc3339();
            
            let _: () = conn.hset_multiple(&new_job_key, &[
                ("queue", queue.as_str()),
                ("status", "pending"),
                ("payload", payload.as_str()),
                ("created_at", &now),
            ]).await?;
            
            let _: () = conn.rpush(&queue_key, &job_id).await?;
            let _: () = conn.sadd("snm:queues", queue).await?;
            
            println!("{}", format!("✅ Cron job triggered. New job ID: {}", job_id).green());
        } else {
            println!("{}", "❌ Invalid cron job data.".red());
        }
    }
    
    Ok(())
}

// Show logs
pub async fn logs_command(matches: &ArgMatches) -> Result<()> {
    let queue = matches.get_one::<String>("queue");
    let follow = matches.get_flag("follow");
    let lines: isize = matches.get_one::<String>("lines").unwrap().parse().unwrap_or(50);
    
    if follow {
        println!("{}", "📜 Following logs (Press Ctrl+C to stop)".blue().bold());
        let mut interval = interval(Duration::from_secs(1));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = show_logs(queue, lines).await {
                        eprintln!("Error: {}", e);
                        break;
                    }
                    println!("{}", "-".repeat(80).blue());
                }
                _ = signal::ctrl_c() => {
                    println!("\n{}", "👋 Stopped following logs.".green());
                    break;
                }
            }
        }
    } else {
        show_logs(queue, lines).await?;
    }
    
    Ok(())
}

async fn show_logs(queue_filter: Option<&String>, lines: isize) -> Result<()> {
    let mut conn = get_redis_connection().await?;
    
    if let Some(queue) = queue_filter {
        let log_key = format!("snm:logs:{}", queue);
        let logs: Vec<String> = conn.lrange(&log_key, 0, lines - 1).await?;
        
        if logs.is_empty() {
            println!("No logs found for queue '{}'.", queue);
        } else {
            println!("{}", format!("📜 Logs for queue '{}' (last {} entries)", queue, logs.len()).blue().bold());
            for log in logs {
                println!("{}", log);
            }
        }
    } else {
        let queues: Vec<String> = conn.smembers("snm:queues").await?;
        for queue in queues {
            let log_key = format!("snm:logs:{}", queue);
            let logs: Vec<String> = conn.lrange(&log_key, 0, 5).await?;
            
            if !logs.is_empty() {
                println!("{}", format!("📜 {} (recent)", queue).blue().bold());
                for log in logs.iter().take(3) {
                    println!("  {}", log);
                }
                if logs.len() > 3 {
                    println!("  ... and {} more", logs.len() - 3);
                }
                println!();
            }
        }
    }
    
    Ok(())
}

// Clear queues/jobs
pub async fn clear_command(matches: &ArgMatches) -> Result<()> {
    let queue = matches.get_one::<String>("queue");
    let clear_failed = matches.get_flag("failed");
    let clear_all = matches.get_flag("all");
    
    let mut conn = get_redis_connection().await?;
    
    if clear_all {
        println!("{}", "🗑️  Clearing all queues...".yellow());
        
        let queues: Vec<String> = conn.smembers("snm:queues").await?;
        let mut cleared_count = 0;
        
        for queue in queues {
            let queue_key = format!("snm:queue:{}", queue);
            let count: usize = conn.llen(&queue_key).await.unwrap_or(0);
            let _: () = conn.del(&queue_key).await?;
            cleared_count += count;
        }
        
        println!("{}", format!("✅ Cleared {} jobs from all queues.", cleared_count).green());
        
    } else if clear_failed {
        println!("{}", "🗑️  Clearing all failed jobs...".yellow());
        
        let failed_jobs: Vec<String> = conn.lrange("snm:failed_jobs", 0, -1).await?;
        let count = failed_jobs.len();
        
        for job_id in failed_jobs {
            let job_key = format!("snm:job:{}", job_id);
            let _: () = conn.del(&job_key).await?;
        }
        
        let _: () = conn.del("snm:failed_jobs").await?;
        println!("{}", format!("✅ Cleared {} failed jobs.", count).green());
        
    } else if let Some(queue) = queue {
        println!("{}", format!("🗑️  Clearing queue '{}'...", queue).yellow());
        
        let queue_key = format!("snm:queue:{}", queue);
        let count: usize = conn.llen(&queue_key).await.unwrap_or(0);
        let _: () = conn.del(&queue_key).await?;
        
        println!("{}", format!("✅ Cleared {} jobs from queue '{}'.", count, queue).green());
    } else {
        println!("Specify --queue, --failed, or --all to clear jobs.");
    }
    
    Ok(())
}

// Start web UI
pub async fn web_command(matches: &ArgMatches) -> Result<()> {
    let port = matches.get_one::<String>("port").unwrap();
    let host = matches.get_one::<String>("host").unwrap();
    
    println!("{}", format!("🌐 Starting QRush Web UI at http://{}:{}", host, port).blue().bold());
    println!("{}", "This would start the web interface...".yellow());
    println!("{}", "Press Ctrl+C to stop.".blue());
    
    // Wait for Ctrl+C
    signal::ctrl_c().await?;
    println!("\n{}", "👋 Web UI stopped.".green());
    
    Ok(())
}
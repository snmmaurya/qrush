// src/bin/qrush.rs
use clap::{Arg, Command};
use std::process;
use anyhow::Result;

mod commands;
use commands::*;

#[tokio::main]
async fn main() -> Result<()> {
    let app = Command::new("qrush")
        .version("0.4.0")
        .author("SNM Maurya <sxmmaurya@gmail.com>")
        .about("QRush - Lightweight Job Queue CLI for Rust")
        .subcommand(
            Command::new("start")
                .about("Start QRush workers")
                .arg(Arg::new("queues")
                    .short('q')
                    .long("queues")
                    .value_name("QUEUE1,QUEUE2")
                    .help("Comma-separated list of queues to process")
                    .default_value("default"))
                .arg(Arg::new("concurrency")
                    .short('c')
                    .long("concurrency")
                    .value_name("NUMBER")
                    .help("Number of worker threads")
                    .default_value("5"))
                .arg(Arg::new("environment")
                    .short('e')
                    .long("environment")
                    .value_name("ENV")
                    .help("Environment (development, production)")
                    .default_value("development"))
                .arg(Arg::new("daemon")
                    .short('d')
                    .long("daemon")
                    .help("Run as daemon")
                    .action(clap::ArgAction::SetTrue))
                .arg(Arg::new("logfile")
                    .short('L')
                    .long("logfile")
                    .value_name("PATH")
                    .help("Path to logfile"))
                .arg(Arg::new("pidfile")
                    .short('P')
                    .long("pidfile")
                    .value_name("PATH")
                    .help("Path to pidfile"))
        )
        .subcommand(
            Command::new("stop")
                .about("Stop QRush workers")
                .arg(Arg::new("timeout")
                    .short('t')
                    .long("timeout")
                    .value_name("SECONDS")
                    .help("Shutdown timeout in seconds")
                    .default_value("25"))
        )
        .subcommand(
            Command::new("restart")
                .about("Restart QRush workers")
                .arg(Arg::new("timeout")
                    .short('t')
                    .long("timeout")
                    .value_name("SECONDS")
                    .help("Shutdown timeout in seconds")
                    .default_value("25"))
        )
        .subcommand(
            Command::new("status")
                .about("Show QRush status and statistics")
                .arg(Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .help("Show detailed information")
                    .action(clap::ArgAction::SetTrue))
        )
        .subcommand(
            Command::new("stats")
                .about("Show queue statistics")
                .arg(Arg::new("queue")
                    .short('q')
                    .long("queue")
                    .value_name("QUEUE")
                    .help("Show stats for specific queue"))
                .arg(Arg::new("watch")
                    .short('w')
                    .long("watch")
                    .help("Watch stats in real-time")
                    .action(clap::ArgAction::SetTrue))
        )
        .subcommand(
            Command::new("queues")
                .about("List all queues")
                .arg(Arg::new("verbose")
                    .short('v')
                    .long("verbose")
                    .help("Show detailed queue information")
                    .action(clap::ArgAction::SetTrue))
        )
        .subcommand(
            Command::new("jobs")
                .about("Manage jobs")
                .subcommand(
                    Command::new("list")
                        .about("List jobs")
                        .arg(Arg::new("queue")
                            .short('q')
                            .long("queue")
                            .value_name("QUEUE")
                            .help("Queue name")
                            .required(true))
                        .arg(Arg::new("status")
                            .short('s')
                            .long("status")
                            .value_name("STATUS")
                            .help("Filter by status (pending, success, failed, delayed)")
                            .default_value("pending"))
                        .arg(Arg::new("limit")
                            .short('l')
                            .long("limit")
                            .value_name("NUMBER")
                            .help("Limit number of results")
                            .default_value("10"))
                )
                .subcommand(
                    Command::new("retry")
                        .about("Retry failed jobs")
                        .arg(Arg::new("job_id")
                            .help("Specific job ID to retry")
                            .required(false))
                        .arg(Arg::new("all")
                            .long("all")
                            .help("Retry all failed jobs")
                            .action(clap::ArgAction::SetTrue))
                )
                .subcommand(
                    Command::new("delete")
                        .about("Delete jobs")
                        .arg(Arg::new("job_id")
                            .help("Specific job ID to delete")
                            .required(false))
                        .arg(Arg::new("failed")
                            .long("failed")
                            .help("Delete all failed jobs")
                            .action(clap::ArgAction::SetTrue))
                )
                .subcommand(
                    Command::new("enqueue")
                        .about("Enqueue a test job")
                        .arg(Arg::new("queue")
                            .short('q')
                            .long("queue")
                            .value_name("QUEUE")
                            .help("Queue name")
                            .default_value("default"))
                        .arg(Arg::new("job_type")
                            .short('t')
                            .long("type")
                            .value_name("TYPE")
                            .help("Job type")
                            .required(true))
                        .arg(Arg::new("payload")
                            .short('p')
                            .long("payload")
                            .value_name("JSON")
                            .help("Job payload as JSON")
                            .default_value("{}"))
                        .arg(Arg::new("delay")
                            .short('d')
                            .long("delay")
                            .value_name("SECONDS")
                            .help("Delay execution by N seconds"))
                )
        )
        .subcommand(
            Command::new("cron")
                .about("Manage cron jobs")
                .subcommand(Command::new("list").about("List all cron jobs"))
                .subcommand(
                    Command::new("enable")
                        .about("Enable a cron job")
                        .arg(Arg::new("job_id")
                            .help("Cron job ID")
                            .required(true))
                )
                .subcommand(
                    Command::new("disable")
                        .about("Disable a cron job")
                        .arg(Arg::new("job_id")
                            .help("Cron job ID")
                            .required(true))
                )
                .subcommand(
                    Command::new("run")
                        .about("Run a cron job immediately")
                        .arg(Arg::new("job_id")
                            .help("Cron job ID")
                            .required(true))
                )
        )
        .subcommand(
            Command::new("logs")
                .about("Show logs")
                .arg(Arg::new("queue")
                    .short('q')
                    .long("queue")
                    .value_name("QUEUE")
                    .help("Show logs for specific queue"))
                .arg(Arg::new("follow")
                    .short('f')
                    .long("follow")
                    .help("Follow logs in real-time")
                    .action(clap::ArgAction::SetTrue))
                .arg(Arg::new("lines")
                    .short('n')
                    .long("lines")
                    .value_name("NUMBER")
                    .help("Number of lines to show")
                    .default_value("50"))
        )
        .subcommand(
            Command::new("clear")
                .about("Clear queues or jobs")
                .arg(Arg::new("queue")
                    .short('q')
                    .long("queue")
                    .value_name("QUEUE")
                    .help("Clear specific queue"))
                .arg(Arg::new("failed")
                    .long("failed")
                    .help("Clear failed jobs")
                    .action(clap::ArgAction::SetTrue))
                .arg(Arg::new("all")
                    .long("all")
                    .help("Clear all queues")
                    .action(clap::ArgAction::SetTrue))
        )
        .subcommand(
            Command::new("web")
                .about("Start web UI")
                .arg(Arg::new("port")
                    .short('p')
                    .long("port")
                    .value_name("PORT")
                    .help("Port to bind web UI")
                    .default_value("4567"))
                .arg(Arg::new("host")
                    .short('h')
                    .long("host")
                    .value_name("HOST")
                    .help("Host to bind web UI")
                    .default_value("127.0.0.1"))
        );

    let matches = app.get_matches();

    match matches.subcommand() {
        Some(("start", sub_matches)) => start_command(sub_matches).await,
        Some(("stop", sub_matches)) => stop_command(sub_matches).await,
        Some(("restart", sub_matches)) => restart_command(sub_matches).await,
        Some(("status", sub_matches)) => status_command(sub_matches).await,
        Some(("stats", sub_matches)) => stats_command(sub_matches).await,
        Some(("queues", sub_matches)) => queues_command(sub_matches).await,
        Some(("jobs", sub_matches)) => jobs_command(sub_matches).await,
        Some(("cron", sub_matches)) => cron_command(sub_matches).await,
        Some(("logs", sub_matches)) => logs_command(sub_matches).await,
        Some(("clear", sub_matches)) => clear_command(sub_matches).await,
        Some(("web", sub_matches)) => web_command(sub_matches).await,
        _ => {
            println!("No command specified. Use --help for usage information.");
            process::exit(1);
        }
    }
}

// Helper function to get Redis connection using qrush config
async fn get_redis_connection() -> Result<redis::aio::MultiplexedConnection> {
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    
    let client = redis::Client::open(redis_url)?;
    let conn = client.get_multiplexed_async_connection().await?;
    Ok(conn)
}
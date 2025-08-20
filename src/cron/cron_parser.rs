// src/cron/cron_parser.rs
use chrono::{DateTime, Utc, Datelike, Timelike};
use anyhow::{Result, anyhow};

/// Simple cron parser
pub struct CronParser;

impl CronParser {
    /// Parse cron expression and calculate next execution time
    /// Format: "sec min hour day month weekday"
    /// Example: "0 */5 * * * *" = every 5 minutes
    pub fn next_execution(cron_expr: &str, from: DateTime<Utc>) -> Result<DateTime<Utc>> {
        let parts: Vec<&str> = cron_expr.split_whitespace().collect();
        if parts.len() != 6 {
            return Err(anyhow!("Invalid cron expression format. Expected 6 parts: sec min hour day month weekday"));
        }

        // Simple implementation for common patterns
        match cron_expr {
            "0 * * * * *" => Ok(from + chrono::Duration::minutes(1)), // Every minute
            "0 */5 * * * *" => Ok(from + chrono::Duration::minutes(5)), // Every 5 minutes
            "0 */10 * * * *" => Ok(from + chrono::Duration::minutes(10)), // Every 10 minutes
            "0 */15 * * * *" => Ok(from + chrono::Duration::minutes(15)), // Every 15 minutes
            "0 */30 * * * *" => Ok(from + chrono::Duration::minutes(30)), // Every 30 minutes
            "0 0 * * * *" => Ok(from + chrono::Duration::hours(1)), // Every hour
            "0 0 0 * * *" => Ok(from + chrono::Duration::days(1)), // Daily at midnight
            "0 0 2 * * *" => {
                // Daily at 2 AM
                let mut next = from + chrono::Duration::days(1);
                next = next.date_naive().and_hms_opt(2, 0, 0).unwrap().and_utc();
                if next <= from {
                    next = next + chrono::Duration::days(1);
                }
                Ok(next)
            },
            "0 0 9 * * *" => {
                // Daily at 9 AM
                let mut next = from.date_naive().and_hms_opt(9, 0, 0).unwrap().and_utc();
                if next <= from {
                    next = next + chrono::Duration::days(1);
                }
                Ok(next)
            },
            "0 0 0 * * 1" => {
                // Every Monday at midnight
                let days_until_monday = (7 - from.weekday().num_days_from_monday()) % 7;
                let next_monday = if days_until_monday == 0 && from.hour() > 0 {
                    from + chrono::Duration::days(7)
                } else {
                    from + chrono::Duration::days(days_until_monday as i64)
                };
                Ok(next_monday.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc())
            },
            "0 0 0 1 * *" => {
                // First day of every month
                let mut next = from.date_naive().with_day(1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
                if next <= from {
                    // Move to next month
                    if next.month() == 12 {
                        next = next.with_year(next.year() + 1).unwrap().with_month(1).unwrap();
                    } else {
                        next = next.with_month(next.month() + 1).unwrap();
                    }
                }
                Ok(next)
            },
            _ => {
                // For complex expressions, you'd implement full cron parsing here
                // or use the `cron` crate
                Err(anyhow!("Cron expression not supported yet: {}. Supported patterns: '0 * * * * *' (every minute), '0 */5 * * * *' (every 5 min), '0 0 * * * *' (hourly), '0 0 0 * * *' (daily), '0 0 0 * * 1' (weekly), '0 0 0 1 * *' (monthly)", cron_expr))
            }
        }
    }
}

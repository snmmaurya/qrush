# QRush CLI Usage Guide

The QRush CLI provides comprehensive command-line management for your job queue, similar to Sidekiq in Rails.

## Installation & Setup

1. Add the CLI dependencies to your `Cargo.toml`
2. Create the binary at `src/bin/qrush.rs`
3. Create the commands module at `src/bin/commands/mod.rs`
4. Build with: `cargo build --release`

## Environment Variables

```bash
export REDIS_URL="redis://127.0.0.1:6379"
export QRUSH_BASIC_AUTH="admin:password"  # Optional web UI auth
```

## Basic Commands

### Start Workers
```bash
# Start with default settings
qrush start

# Start with specific queues and concurrency
qrush start -q default,critical -c 10

# Start as daemon with log file
qrush start -d -L /var/log/qrush.log -P /var/run/qrush.pid

# Start for specific environment
qrush start -e production
```

### Stop Workers
```bash
# Graceful shutdown
qrush stop

# Stop with custom timeout
qrush stop -t 30
```

### Restart Workers
```bash
qrush restart
```

## Monitoring Commands

### Check Status
```bash
# Basic status
qrush status

# Detailed status with worker info
qrush status -v
```

### View Statistics
```bash
# Show all queue stats
qrush stats

# Show stats for specific queue
qrush stats -q critical

# Watch stats in real-time
qrush stats -w
```

### List Queues
```bash
# Simple list
qrush queues

# Detailed queue information
qrush queues -v
```

## Job Management

### List Jobs
```bash
# List pending jobs in a queue
qrush jobs list -q default

# List failed jobs
qrush jobs list -q default -s failed

# List with custom limit
qrush jobs list -q default -l 20
```

### Retry Jobs
```bash
# Retry specific job
qrush jobs retry abc123def

# Retry all failed jobs
qrush jobs retry --all
```

### Delete Jobs
```bash
# Delete specific job
qrush jobs delete abc123def

# Delete all failed jobs
qrush jobs delete --failed
```

### Enqueue Test Jobs
```bash
# Enqueue immediate job
qrush jobs enqueue -q default -t NotifyUser -p '{"user_id": "123", "message": "Hello"}'

# Enqueue delayed job
qrush jobs enqueue -q default -t NotifyUser -p '{"user_id": "123"}' -d 60
```

## Cron Job Management

### List Cron Jobs
```bash
qrush cron list
```

### Enable/Disable Cron Jobs
```bash
# Enable a cron job
qrush cron enable daily_report

# Disable a cron job
qrush cron disable daily_report
```

### Run Cron Job Immediately
```bash
qrush cron run daily_report
```

## Logs & Debugging

### View Logs
```bash
# Show recent logs for all queues
qrush logs

# Show logs for specific queue
qrush logs -q default

# Follow logs in real-time
qrush logs -f

# Show more lines
qrush logs -n 100
```

## Cleanup Commands

### Clear Jobs
```bash
# Clear specific queue
qrush clear -q default

# Clear all failed jobs
qrush clear --failed

# Clear all queues (dangerous!)
qrush clear --all
```

## Web UI

### Start Web Interface
```bash
# Start on default port 4567
qrush web

# Start on custom port and host
qrush web -p 8080 -h 0.0.0.0
```

## Advanced Usage Examples

### Production Deployment
```bash
# Start production workers
qrush start -e production -q critical,default,low -c 20 -d -L /var/log/qrush.log

# Monitor in production
qrush status -v
qrush stats -w
```

### Development Workflow
```bash
# Start development workers
qrush start -e development -q default -c 5

# Monitor jobs
qrush jobs list -q default
qrush logs -f

# Test job enqueueing
qrush jobs enqueue -q default -t TestJob -p '{}'
```

### Debugging Failed Jobs
```bash
# View failed jobs
qrush jobs list -q default -s failed

# Check logs for errors
qrush logs -q default -n 100

# Retry failed jobs after fixes
qrush jobs retry --all
```

### Cron Job Management
```bash
# List all scheduled jobs
qrush cron list

# Temporarily disable a job
qrush cron disable weekly_cleanup

# Run a job manually for testing
qrush cron run weekly_cleanup

# Re-enable the job
qrush cron enable weekly_cleanup
```

## Integration with Systemd

Create `/etc/systemd/system/qrush.service`:

```ini
[Unit]
Description=QRush Job Queue
After=redis.service
Requires=redis.service

[Service]
Type=simple
User=qrush
WorkingDirectory=/opt/myapp
Environment=REDIS_URL=redis://127.0.0.1:6379
Environment=RUST_LOG=info
ExecStart=/opt/myapp/target/release/qrush start -e production -q critical,default -c 10
ExecStop=/opt/myapp/target/release/qrush stop -t 30
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable qrush
sudo systemctl start qrush
sudo systemctl status qrush
```

## Command Aliases

Add to your `.bashrc` or `.zshrc`:

```bash
alias qs='qrush status -v'
alias qw='qrush stats -w'
alias ql='qrush logs -f'
alias qj='qrush jobs list'
```

## Tips & Best Practices

1. **Always check status before stopping**: `qrush status -v`
2. **Use graceful shutdowns**: Don't kill workers forcefully
3. **Monitor logs regularly**: `qrush logs -f`
4. **Clear failed jobs periodically**: `qrush clear --failed`
5. **Use appropriate concurrency**: Start with low numbers and scale up
6. **Set up monitoring**: Use `qrush stats -w` during high load
7. **Test cron jobs manually**: `qrush cron run job_id` before enabling

This CLI gives you complete control over your QRush job queue, making it easy to deploy, monitor, and debug in any environment!
// src/rdconfig.rs
use redis::{aio::MultiplexedConnection, Client};
use crate::config::get_redis_url;

pub async fn get_redis_conn() -> redis::RedisResult<MultiplexedConnection> {
    let redis_url = get_redis_url();

    // Client::open will auto-handle rediss:// if TLS feature is enabled
    let client = Client::open(redis_url)?;
    client.get_multiplexed_async_connection().await
}

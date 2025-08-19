// src/utils/rdconfig.rs
use redis::{aio::MultiplexedConnection, Client}; // Keep MultiplexedConnection
use crate::config::get_redis_url;

pub async fn get_redis_connection() -> redis::RedisResult<MultiplexedConnection> {
    let redis_url = get_redis_url();
    let client = Client::open(redis_url)?;
    client.get_multiplexed_async_connection().await
}
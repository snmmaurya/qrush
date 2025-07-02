use redis::{aio::MultiplexedConnection, Client};
use crate::config::get_redis_url;

pub async fn get_redis_conn() -> redis::RedisResult<MultiplexedConnection> {
    let client = Client::open(get_redis_url())?;
    client.get_multiplexed_async_connection().await
}

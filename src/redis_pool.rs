use redis::{aio::Connection, Client};
use crate::config::get_redis_url;

pub async fn get_redis_conn() -> redis::RedisResult<Connection> {
    let client = Client::open(get_redis_url())?;
    client.get_async_connection().await
}

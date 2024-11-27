use bb8_redis::RedisConnectionManager;
use clap::Parser;

use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use tokio_postgres::NoTls;

mod controller;
mod service;
mod test_client;

#[cfg(test)]
mod tests;

#[derive(clap::Parser)]
#[command(version, about, long_about = None)]
/// A simple restaurant REST API
struct Args {
    #[arg(default_value_t = 3000)]
    /// Port the web API server will listen on
    port: u16,

    #[arg(long, short, default_value = "host=localhost port=5432 user=postgres password=postgres dbname=restaurant")]
    /// Connection string used for connecting to PostgreSQL
    postgres_connection_string: String,

    #[arg(long, short, default_value = "redis://localhost")]
    /// Connection string used for connecting to Redis
    redis_connection_string: String
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    let postgres_pool = Pool::builder()
        .build(PostgresConnectionManager::new_from_stringlike(&args.postgres_connection_string, NoTls).unwrap())
        .await
        .unwrap();

    let manager = RedisConnectionManager::new(args.redis_connection_string).unwrap();
    let redis_pool = Pool::builder().build(manager).await.unwrap();

    let app = controller::setup_router(postgres_pool, redis_pool).await;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", args.port)).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}


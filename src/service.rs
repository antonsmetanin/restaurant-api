use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use bb8_redis::RedisConnectionManager;
use chrono::{DateTime, TimeDelta, Utc};
use redis::RedisError;
use tokio_postgres::NoTls;
use const_format::concatcp;

type DishId = i32;
type TableId = i32;
type OrderId = i32;

#[derive(Clone)]
pub struct Order {
    pub id: OrderId,
    pub dish_id: DishId,
    pub ready_time: DateTime<Utc>
}

pub enum ServiceError {
    DatabaseConnection(bb8::RunError<tokio_postgres::Error>),
    RedisConnection(bb8::RunError<RedisError>),
    DatabaseQuery(tokio_postgres::Error),
    RedisQuery(RedisError),
    NotFound,
    BadHeader,
    Bug(String)
}

impl From<bb8::RunError<tokio_postgres::Error>> for ServiceError {
    fn from(value: bb8::RunError<tokio_postgres::Error>) -> Self {
        Self::DatabaseConnection(value)
    }
}

impl From<bb8::RunError<RedisError>> for ServiceError {
    fn from(value: bb8::RunError<RedisError>) -> Self {
        Self::RedisConnection(value)
    }
}

impl From<tokio_postgres::Error> for ServiceError {
    fn from(value: tokio_postgres::Error) -> Self {
        Self::DatabaseQuery(value)
    }
}

impl From<RedisError> for ServiceError {
    fn from(value: RedisError) -> Self {
        Self::RedisQuery(value)
    }
}

#[derive(Clone)]
pub struct State {
    pub postgres_pool: Pool<PostgresConnectionManager<NoTls>>,
    pub redis_pool: Pool<RedisConnectionManager>
}

impl State {
    pub async fn add_order(
        &self,
        table_id: TableId,
        dish_id: DishId
    ) -> Result<Order, ServiceError> {
        let db = self.postgres_pool.get().await?;
        let ready_time = Utc::now() + TimeDelta::minutes(15);

        let result = db.query(
            "INSERT INTO orders (id, table_id, dish_id, ready_time) VALUES (DEFAULT, $1, $2, $3) RETURNING id;",
            &[&table_id, &dish_id, &ready_time]
        ).await?;

        let id: OrderId = result[0].get(0);

        Ok(Order {
            id,
            dish_id,
            ready_time
        })
    }

    pub async fn get_order(
        &self,
        table_id: TableId,
        order_id: OrderId
    ) -> Result<Order, ServiceError> {
        let db = self.postgres_pool.get().await?;

        let rows = db.query(
            "SELECT dish_id, ready_time FROM orders WHERE id = $1 AND table_id = $2 AND deleted = false;",
            &[&order_id, &table_id]
        ).await?;

        let row = rows.first().ok_or(ServiceError::NotFound)?;

        Ok(Order {
            id: order_id,
            dish_id: row.get(0),
            ready_time: row.get(1)
        })
    }

    pub async fn get_orders(
        &self,
        table_id: TableId,
        from_id: Option<i32>,
        limit: Option<i32>
    ) -> Result<Vec<Order>, ServiceError> {
        let db = self.postgres_pool.get().await?;

        const QUERY_STRING: &str = "SELECT id, dish_id, ready_time FROM orders WHERE table_id = $1 AND deleted = false";

        let orders = match (from_id, limit) {
            (Some(from_id), Some(limit)) => db.query(
                concatcp!(QUERY_STRING, " AND id >= $2 ORDER BY id LIMIT $3;"),
                &[&table_id, &from_id, &(limit as i64)]
            ).await,
            (Some(from_id), None) => db.query(
                concatcp!(QUERY_STRING, " AND id >= $2 ORDER BY id;"),
                &[&table_id, &from_id]
            ).await,
            (None, Some(limit)) => db.query(
                concatcp!(QUERY_STRING, " ORDER BY id LIMIT $2;"),
                &[&table_id, &(limit as i64)]
            ).await,
            (None, None) => db.query(
                concatcp!(QUERY_STRING, ";"),
                &[&table_id]
            ).await
        }?;

        Ok(orders.iter().map(|row| Order {
            id: row.get(0),
            dish_id: row.get(1),
            ready_time: row.get(2)
        }).collect())
    }

    pub async fn delete_order(&self, table_id: TableId, order_id: OrderId) -> Result<(), ServiceError> {
        let db = self.postgres_pool.get().await?;

        let rows_updated = db.execute(
            "UPDATE orders SET deleted = true WHERE id = $1 AND table_id = $2 AND deleted = false;",
            &[&order_id, &table_id]
        ).await?;

        match rows_updated {
            0 => Err(ServiceError::NotFound),
            1 => Ok(()),
            x => Err(ServiceError::Bug(format!("Delete updated {} rows", x)))
        }
    }
}
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json,
    Router
};
use bb8_redis::RedisConnectionManager;
use chrono::{DateTime, Utc};
use chrono::serde::ts_seconds;
use redis::AsyncCommands;
use tokio_postgres::NoTls;
use crate::service::ServiceError;
use serde::{Deserialize, Serialize};

use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use tower_http::trace::TraceLayer;

type DishId = i32;
type TableId = i32;
type OrderId = i32;

type AppState = crate::service::State;

pub async fn setup_router(connection_pool: Pool<PostgresConnectionManager<NoTls>>, redis_pool: Pool<RedisConnectionManager>) -> Router {
    let state = AppState { postgres_pool: connection_pool, redis_pool };

    Router::new()
        .route("/health", get(health))
        .nest("/v1", Router::new()
            .layer(TraceLayer::new_for_http())
            .route("/tables/:table_id/orders", post(create_order))
            .route("/tables/:table_id/orders", get(get_orders))
            .route("/tables/:table_id/orders/:order_id", delete(delete_order))
            .route("/tables/:table_id/orders/:order_id", get(get_order))
            .with_state(state)
        )
}

#[derive(Deserialize)]
struct CreateOrder {
    dish_id: DishId
}

#[derive(Deserialize)]
struct Pagination {
    from_id: Option<u32>,
    limit: Option<u32>
}

#[derive(Serialize, Clone)]
pub struct Order {
    id: OrderId,
    dish_id: DishId,
    #[serde(with = "ts_seconds")]
    ready_time: DateTime<Utc>
}

impl From<&crate::service::Order> for Order {
    fn from(value: &crate::service::Order) -> Self {
        Self {
            id: value.id,
            dish_id: value.dish_id,
            ready_time: value.ready_time
        }
    }
}

impl From<crate::service::Order> for Order {
    fn from(value: crate::service::Order) -> Self {
        From::<&crate::service::Order>::from(&value)
    }
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn create_order(
    State(state): State<AppState>,
    Path(table_id): Path<TableId>,
    headers: HeaderMap,
    Json(CreateOrder { dish_id }): Json<CreateOrder>
) -> Result<(StatusCode, String), ServiceError> {
    let idempotency_key = headers.get("Idempotency-Key")
        .map(|x| x.to_str()).transpose().map_err(|_| ServiceError::BadHeader)?;

    let mut cache = match idempotency_key {
        Some(key) => Some((
            state.redis_pool.get().await?,
            format!("{}_{}_{}", table_id, dish_id, key)
        )),
        None => None
    };

    if let Some((cache, cache_key)) = &mut cache {
        if let Ok(cache_response) = cache.get(&cache_key).await {
            return Ok((StatusCode::CREATED, cache_response))
        }
    }

    let order: Order = state.add_order(table_id, dish_id)
        .await?
        .into();

    let json = serde_json::to_string(&order).unwrap();

    if let Some((cache, cache_key)) = &mut cache {
        let _: () = cache.set_ex(&cache_key, &json, 600).await?;
    }

    Ok((StatusCode::CREATED, json))
}

async fn delete_order(
    State(state): State<AppState>,
    Path((table_id, order_id)): Path<(TableId, OrderId)>
) -> Result<StatusCode, ServiceError> {
    state.delete_order(table_id, order_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_order(
    State(state): State<AppState>,
    Path((table_id, order_id)): Path<(TableId, OrderId)>
) -> Result<Json<Order>, ServiceError> {
    let order = state.get_order(table_id, order_id)
        .await?
        .into();

    Ok(Json(order))
}

async fn get_orders(
    State(state): State<AppState>,
    Path(table_id): Path<TableId>,
    Query(Pagination { from_id, limit }): Query<Pagination>,
) -> Result<Json<Vec<Order>>, ServiceError> {
    let orders: Vec<Order> = state
        .get_orders(
            table_id,
            from_id.map(|x| x as i32),
            limit.map(|x| x as i32)
        )
        .await?
        .iter()
        .map(|x| x.into())
        .collect();

    Ok(Json(orders))
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        match self {
            ServiceError::NotFound => StatusCode::NOT_FOUND,
            ServiceError::BadHeader => StatusCode::BAD_REQUEST,
            ServiceError::DatabaseConnection(error) => {
                tracing::error!("{}", &error);
                StatusCode::SERVICE_UNAVAILABLE
            }
            ServiceError::RedisConnection(error) => {
                tracing::error!("{}", &error);
                StatusCode::SERVICE_UNAVAILABLE
            }
            ServiceError::DatabaseQuery(error) => {
                tracing::error!("{}", &error);
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ServiceError::RedisQuery(error) => {
                tracing::error!("{}", &error);
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ServiceError::Bug(error) => {
                tracing::error!("{}", &error);
                StatusCode::INTERNAL_SERVER_ERROR
            }            
        }.into_response()
    }
}
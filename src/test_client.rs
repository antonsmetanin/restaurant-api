use chrono::{serde::ts_seconds, DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub struct TestClient {
    client: reqwest::Client,
    base_url: reqwest::Url
}

type OrderId = i32;
type TableId = i32;
type DishId = i32;

#[derive(Serialize)]
struct CreateOrder {
    dish_id: DishId
}

#[derive(Deserialize, PartialEq, Eq, Debug)]
pub struct Order {
    pub id: OrderId,
    pub dish_id: DishId,
    #[serde(with = "ts_seconds")]
    pub ready_time: DateTime<Utc>
}

impl TestClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: reqwest::Url::parse(base_url).unwrap()
        }
    }

    pub async fn create_order(
        &self,
        table_id: TableId,
        dish_id: DishId
    ) -> Result<Order, Box<dyn std::error::Error>> {
        self.create_order_with_idempotency_key(table_id, dish_id, Uuid::new_v4()).await
    }

    pub async fn create_order_with_idempotency_key(
        &self,
        table_id: TableId,
        dish_id: DishId,
        idempotency_key: uuid::Uuid
    ) -> Result<Order, Box<dyn std::error::Error>> {
        Ok(self.client.post(self.base_url.join(&format!("/v1/tables/{}/orders", table_id)).unwrap())
            .json(&CreateOrder { dish_id: 10 })
            .header("Idempotency-Key", idempotency_key.to_string())
            .send()
            .await?
            .json()
            .await?
        )
    }

    pub async fn get_orders(
        &self,
        table_id: TableId
    ) -> Result<Vec<Order>, Box<dyn std::error::Error>> {
        Ok(self.client.get(self.base_url.join(&format!("/v1/tables/{}/orders", table_id)).unwrap())
            .send()
            .await?
            .json()
            .await?
        )
    }

    pub async fn get_orders_paged(
        &self,
        table_id: TableId,
        first_id: OrderId,
        limit: i64
    ) -> Result<Vec<Order>, Box<dyn std::error::Error>> {
        Ok(self.client.get(self.base_url.join(&format!("/v1/tables/{}/orders?from_id={}&limit={}", table_id, first_id, limit)).unwrap())
            .send()
            .await?
            .json()
            .await?
        )
    }

    pub async fn remove_order(
        &self,
        table_id: TableId,
        order_id: OrderId
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.client.delete(self.base_url.join(&format!("/v1/tables/{}/orders/{}", table_id, order_id)).unwrap())
            .send()
            .await?;

        Ok(())
    }
}
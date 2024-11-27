use std::sync::atomic::AtomicI32;

use bb8::Pool;
use bb8_postgres::PostgresConnectionManager;
use bb8_redis::RedisConnectionManager;
use tokio::sync::OnceCell;
use tokio_postgres::NoTls;
use uuid::Uuid;

use crate::test_client::TestClient;

const POSTGRES_CONNECTION_STRING: &str = "host=localhost port=5432 user=postgres password=postgres dbname=test_restaurant";
const REDIS_CONNECTION_STRING: &str = "redis://localhost";
const LISTEN_PORT: u16 = 3000;

static ONCE: OnceCell<()> = OnceCell::const_new();

async fn setup_tests() {
    ONCE.get_or_init(|| async {
        tracing_subscriber::fmt::init();

        let postgres_pool = Pool::builder()
            .build(PostgresConnectionManager::new_from_stringlike(POSTGRES_CONNECTION_STRING, NoTls).unwrap())
            .await
            .unwrap();

        {
            let db = postgres_pool.get().await.unwrap();
            db.execute("DELETE FROM orders", &[]).await.unwrap();
        }

        let manager = RedisConnectionManager::new(REDIS_CONNECTION_STRING).unwrap();
        let redis_pool = Pool::builder().build(manager).await.unwrap();
    
        let app = crate::controller::setup_router(postgres_pool, redis_pool).await;
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", LISTEN_PORT)).await.unwrap();

        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    }).await;
}

static NEXT_TABLE_ID: AtomicI32 = AtomicI32::new(0);

fn next_table_id() -> i32 {
    NEXT_TABLE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

fn new_client() -> TestClient {
    TestClient::new(&format!("http://localhost:{}", LISTEN_PORT))
}

#[tokio_shared_rt::test(shared)]
async fn order_creation_works() {
    setup_tests().await;
    let client = new_client();
    let table_id = next_table_id();

    let order1 = client.create_order(table_id, 10).await.unwrap();
    let order2 = client.create_order(table_id, 10).await.unwrap();
    assert_ne!(order1.id, order2.id);

    let orders = client.get_orders(table_id).await.unwrap();
    assert_eq!(2, orders.len());
    assert!(orders.contains(&order1));
    assert!(orders.contains(&order2));
}

#[tokio_shared_rt::test(shared)]
async fn order_creation_is_idempotent() {
    setup_tests().await;
    let client = new_client();
    let table_id = next_table_id();

    let idempotency_key = Uuid::new_v4();
    let order1 = client.create_order_with_idempotency_key(table_id, 10, idempotency_key).await.unwrap();
    let order2 = client.create_order_with_idempotency_key(table_id, 10, idempotency_key).await.unwrap();
    assert_eq!(order1.id, order2.id);

    let orders = client.get_orders(table_id).await.unwrap();
    assert_eq!(1, orders.len());
    assert!(orders.contains(&order1));
    assert!(orders.contains(&order2));
}

#[tokio_shared_rt::test(shared)]
async fn table_orders_are_kept_separate() {
    setup_tests().await;
    let client = new_client();
    let table1_id = next_table_id();
    let table2_id = next_table_id();

    let order1 = client.create_order(table1_id, 10).await.unwrap();
    let order2 = client.create_order(table2_id, 11).await.unwrap();
    let order3 = client.create_order(table1_id, 6).await.unwrap();
    let order4 = client.create_order(table1_id, 2).await.unwrap();

    let table1_orders = client.get_orders(table1_id).await.unwrap();
    let table2_orders = client.get_orders(table2_id).await.unwrap();
    
    assert_eq!(3, table1_orders.len());
    assert_eq!(1, table2_orders.len());

    assert!(table1_orders.contains(&order1));
    assert!(table1_orders.contains(&order3));
    assert!(table1_orders.contains(&order4));
    assert!(table2_orders.contains(&order2));
}

#[tokio_shared_rt::test(shared)]
async fn order_removal_works() {
    setup_tests().await;
    let client = new_client();
    let table_id = next_table_id();

    let order1 = client.create_order(table_id, 10).await.unwrap();
    let order2 = client.create_order(table_id, 20).await.unwrap();

    client.remove_order(table_id, order1.id).await.unwrap();

    let orders = client.get_orders(table_id).await.unwrap();
    assert_eq!(1, orders.len());

    assert_eq!(&order2, &orders[0]);
}

#[tokio_shared_rt::test(shared)]
async fn order_removal_is_idempotent() {
    setup_tests().await;
    let client = new_client();
    let table_id = next_table_id();

    let order1 = client.create_order(table_id, 10).await.unwrap();
    let order2 = client.create_order(table_id, 20).await.unwrap();

    client.remove_order(table_id, order1.id).await.unwrap();
    client.remove_order(table_id, order1.id).await.unwrap();

    let orders = client.get_orders(table_id).await.unwrap();
    assert_eq!(1, orders.len());

    assert_eq!(&order2, &orders[0]);
}

#[tokio_shared_rt::test(shared)]
async fn order_pagination_works() {
    setup_tests().await;
    let client = new_client();
    let table_id = next_table_id();

    let order1 = client.create_order(table_id, 10).await.unwrap();
    let order2 = client.create_order(table_id, 20).await.unwrap();
    let order3 = client.create_order(table_id, 15).await.unwrap();
    let order4 = client.create_order(table_id, 6).await.unwrap();
    let order5 = client.create_order(table_id, 23).await.unwrap();
    let order6 = client.create_order(table_id, 31).await.unwrap();
    let order7 = client.create_order(table_id, 5).await.unwrap();
    let order8 = client.create_order(table_id, 3).await.unwrap();
    let order9 = client.create_order(table_id, 13).await.unwrap();

    client.remove_order(table_id, order3.id).await.unwrap();
    client.remove_order(table_id, order6.id).await.unwrap();

    let orders = client.get_orders(table_id).await.unwrap();
    assert_eq!(7, orders.len());

    let order_page1 = client.get_orders_paged(table_id, 0, 3).await.unwrap();
    assert_eq!(3, order_page1.len());
    assert!(order_page1.contains(&order1));
    assert!(order_page1.contains(&order2));
    assert!(order_page1.contains(&order4));

    let order_page2 = client.get_orders_paged(table_id, order_page1.last().unwrap().id + 1, 3).await.unwrap();
    assert_eq!(3, order_page2.len());
    assert!(order_page2.contains(&order5));
    assert!(order_page2.contains(&order7));
    assert!(order_page2.contains(&order8));

    let order_page3 = client.get_orders_paged(table_id, order_page2.last().unwrap().id + 1, 3).await.unwrap();
    assert_eq!(1, order_page3.len());
    assert!(order_page3.contains(&order9));
}
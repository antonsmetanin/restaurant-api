A simple restaurant REST API
===

Overview
---
The API lets its users create orders in a restaurant, cancel these orders, and get the list of current orders for specific restaurant tables.

The API follows the REST principle: the `/v1/tables/:table_id/orders` endpoint represents the list of orders for a single table defined by `table_id`. A `GET` request on this endpoint will return the list of current orders, while a `POST` request will create a new one.

The `POST` request optionally accepts an `Idempotency-Key` header, allowing a client retry the request without creating duplicated orders, as long as the idempotency key is the same for each retry and they are made within 10 minutes of the first one.

The `GET` request returns all current orders for a table by default, but also allows pagination by specifying `?from_id=` and/or `?limit=`. The client is expected to use the highest ID received as part of a page plus one as `from_id` in order to get the next page.

Orders can also be received separately by invoking a `/v1/tables/:table_id/orders/:order_id` endpoint.

Each order has a `ready_time` timestamp, which the client can compare with the current time and display the order as either "READY" or "PREPARING" in some UI, if needed.

Since orders for one restaurant table are independent from those for the other tables, the `orders` database table can probably be sharded easily.

The implementation has a `TestClient` struct which is used in tests, but can also be used manually, running test clients on separate threads, for example.

Requirements
---

By default the server expects a [PostgreSQL](https://www.postgresql.org/) server with user=postgres and password=postgres running on localhost,
but a different connection string can be specified using the `-p` command line argument.

Example: `-p "host=localhost port=5432 user=postgres password=postgres dbname=restaurant"`

In the same way it expects [Redis](https://redis.io/) to be running on localhost with connection string specified by the `-r` flag.

Example: `-r "redis://localhost"`

There are two scripts: [setup_db.sql](setup_db.sql) and [setup_test_db.sql](setup_test_db.sql) that create the main database and the test database respectively, along with the tables and indexes. Make sure to run them against the database before starting the server.

An example of how they can be imported:

`psql -U postgres -a -f setup_db.sql`

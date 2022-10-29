use std::env;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use hugsqlx::{params, HugSqlx};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
struct User {
    user_id: i32,
    email: String,
    name: String,
    picture: String,
}

#[derive(HugSqlx)]
#[queries = "examples/resources/queries.sql"]
struct Users {}

fn generate_email(name: &str) -> String {
    format!("{}-{}@foo.com", name, Uuid::new_v4())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;
    let email = generate_email("janko");

    // insert single user and fetch data back from DB
    let result = Users::add_user(
        &pool,
        params!(&email, "Janko Muzykant", "http://my.profile.image.com"),
    )
    .await?;
    if let Some(user) = Users::fetch_user_by_email::<_, User>(&pool, params!(email)).await? {
        println!("Stored user {:?}", user);
    } else {
        eprintln!("Somefink went really wrong. Insertion result: {:?}", result);
    }

    // ok, let's insert few more users
    let results = vec![
        ("Alice", "http://profile-1.com"),
        ("Bob", "http://profile-2.com"),
        ("Mallory", "http://profile-3.com"),
    ]
    .iter()
    .map(|(name, picture)| Users::add_user(&pool, params!(generate_email(name), name, picture)))
    .collect::<FuturesOrdered<_>>()
    .collect::<Vec<_>>()
    .await;

    assert_eq!(results.len(), 3);

    Ok(())
}

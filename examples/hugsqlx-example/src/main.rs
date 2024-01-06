use std::env;

use futures::stream::FuturesOrdered;
use futures::StreamExt;
use hugsqlx::{params, HugSqlx};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct User {
    user_id: i32,
    email: String,
    name: String,
    picture: String,
}

#[derive(Debug)]
#[allow(dead_code)]
struct Profile {
    pub name: String,
    pub picture: String,
}

#[derive(HugSqlx)]
#[queries = "resources/queries.sql"]
struct Users {}

fn generate_email(name: &str) -> String {
    format!("{}-{}@foo.com", name, Uuid::new_v4())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;

    // insert few testing users
    let results = vec![
        ("Alice", "http://profile-1.com"),
        ("Bob", "http://profile-2.com"),
        ("Mallory", "http://profile-3.com"),
    ]
    .into_iter()
    .map(|(name, picture)| Users::add_user(&pool, params!(generate_email(name), name, picture)))
    .collect::<FuturesOrdered<_>>()
    .collect::<Vec<Result<SqliteRow, _>>>()
    .await;

    assert_eq!(results.len(), 3);

    if let Ok(row) = results.first().unwrap() {
        let id = row.try_get::<i64, _>(0).unwrap();
        let email = row.try_get::<String, _>(1).unwrap();

        // fetch first created user
        if let Some(user) = Users::fetch_user_by_email::<_, User>(&pool, params!(email)).await? {
            println!("Stored user => {:?}", user);
        } else {
            eprintln!("Somefink went really wrong...");
        }

        // show user's profile
        let profile: Result<Profile, sqlx::Error> =
            Users::fetch_user_profile(&pool, params!(id), |row: SqliteRow| {
                let name = row.try_get::<String, _>("name")?;
                let picture = row.try_get::<String, _>("picture")?;

                Ok(Profile { name, picture })
            })
            .await?;

        println!("User's profile => {:?}", profile);

        // delete user at the end
        let deletion = Users::delete_user_by_id(&pool, params!(id)).await?;
        println!("User's deletion result => {:?}", deletion);
    }
    Ok(())
}

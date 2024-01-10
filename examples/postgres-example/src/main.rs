use futures::TryStreamExt;
use hugsqlx::{params, HugSqlx};
use sqlx::Row;
use sqlx::{postgres::PgRow, PgPool};
use std::env;

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
#[allow(dead_code)]
struct User {
    user_id: i32,
    email: String,
    name: String,
    picture: String,
}

#[derive(HugSqlx)]
#[queries = "resources/queries.sql"]
struct Users {}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = PgPool::connect(&env::var("DATABASE_URL")?).await?;

    let create_table_result = Users::execute_create_table(&pool, params!()).await?;
    println!("Create table result: {create_table_result:?}");

    for (uid, email, name, pic) in [
        (1, "alice@example.com", "Alice", "alice.png"),
        (2, "bob@example.com", "Robert", "robert.png"),
        (3, "charlie@example.com", "Charlie", "charlie.png"),
        (4, "dick@example.com", "Richard", "richard.png"),
    ] {
        Users::execute_insert_user(&pool, params!(uid, email, name, pic)).await?;
    }
    let expected_users = vec![
        User {
            user_id: 1,
            email: "alice@example.com".to_string(),
            name: "Alice".to_string(),
            picture: "alice.png".to_string(),
        },
        User {
            user_id: 2,
            email: "bob@example.com".to_string(),
            name: "Robert".to_string(),
            picture: "robert.png".to_string(),
        },
        User {
            user_id: 3,
            email: "charlie@example.com".to_string(),
            name: "Charlie".to_string(),
            picture: "charlie.png".to_string(),
        },
        User {
            user_id: 4,
            email: "dick@example.com".to_string(),
            name: "Richard".to_string(),
            picture: "richard.png".to_string(),
        },
    ];

    untyped_example(&pool, &expected_users).await?;
    mapped_example(&pool, &expected_users).await?;
    typed_example(&pool, &expected_users).await?;

    let users = Users::typed_get_multiple_users::<_, User>(&pool, params!()).await?;
    println!("users before drop: {users:?}");

    let drop_table_result = Users::execute_drop_table(&pool, params!()).await?;
    println!("Drop table result: {drop_table_result:?}");

    Ok(())
}

async fn untyped_example(pool: &PgPool, expected: &[User]) -> anyhow::Result<()> {
    let row = Users::untyped_get_user_by_id(pool, params!(1)).await?;
    let got = User {
        user_id: row.try_get("user_id")?,
        email: row.try_get("email")?,
        name: row.try_get("name")?,
        picture: row.try_get("picture")?,
    };
    assert_eq!(got, expected[0]);

    let row = Users::untyped_get_user_by_name(pool, params!("Name_Not_exist")).await?;
    assert!(row.is_none());

    let mut i = 0;
    let rows = Users::untyped_get_multiple_users(pool, params!()).await?;
    for row in rows {
        let got = User {
            user_id: row.try_get("user_id")?,
            email: row.try_get("email")?,
            name: row.try_get("name")?,
            picture: row.try_get("picture")?,
        };
        assert_eq!(got, expected[i]);
        i += 1;
    }
    i = 0;
    let mut rows = Users::untyped_get_stream_users(pool, params!()).await;
    while let Some(row) = rows.try_next().await? {
        let got = User {
            user_id: row.try_get("user_id")?,
            email: row.try_get("email")?,
            name: row.try_get("name")?,
            picture: row.try_get("picture")?,
        };
        assert_eq!(got, expected[i]);
        i += 1;
    }
    Ok(())
}

fn user_mapper(row: PgRow) -> User {
    User {
        user_id: row.get("user_id"),
        email: row.get("email"),
        name: row.get("name"),
        picture: row.get("picture"),
    }
}

async fn mapped_example(pool: &PgPool, expected: &[User]) -> anyhow::Result<()> {
    let got = Users::mapped_get_user_by_id(pool, params!(1), user_mapper).await?;
    assert_eq!(got, expected[0]);

    let row = Users::mapped_get_user_by_name(pool, params!("Name_Not_exist"), user_mapper).await?;
    assert!(row.is_none());

    let mut i = 0;
    let rows = Users::mapped_get_multiple_users(pool, params!(), user_mapper).await?;
    for got in rows {
        assert_eq!(got, expected[i]);
        i += 1;
    }
    i = 0;
    let mut rows = Users::mapped_get_stream_users(pool, params!(), user_mapper).await;
    while let Some(got) = rows.try_next().await? {
        assert_eq!(got, expected[i]);
        i += 1;
    }
    Ok(())
}

async fn typed_example(pool: &PgPool, expected: &[User]) -> anyhow::Result<()> {
    let got = Users::typed_get_user_by_id::<_, User>(pool, params!(1)).await?;
    assert_eq!(got, expected[0]);

    let row = Users::typed_get_user_by_name::<_, User>(pool, params!("Name_Not_exist")).await?;
    assert!(row.is_none());

    let mut i = 0;
    let rows = Users::typed_get_multiple_users::<_, User>(pool, params!()).await?;
    for got in rows {
        assert_eq!(got, expected[i]);
        i += 1;
    }
    i = 0;
    let mut rows = Users::typed_get_stream_users::<_, User>(pool, params!()).await;
    while let Some(got) = rows.try_next().await? {
        assert_eq!(got, expected[i]);
        i += 1;
    }
    Ok(())
}

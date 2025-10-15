use common::{expected_users, sample_data, User};
use futures::TryStreamExt;
use hugsqlx::{params, HugSqlx};
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};
use std::env;

#[derive(HugSqlx)]
#[queries = "../common/resources/queries.sql"]
struct Users {}

fn user_mapper(row: SqliteRow) -> User {
    User {
        user_id: row.get("user_id"),
        email: row.get("email"),
        name: row.get("name"),
        picture: row.get("picture"),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let pool = SqlitePool::connect(&env::var("DATABASE_URL")?).await?;

    Users::execute_create_table(&pool, params!()).await?;
    println!("Users table created. Feeding with sample data...");

    for (uid, email, name, pic) in sample_data() {
        Users::execute_insert_user(&pool, params!(uid, email, name, pic)).await?;
    }
    let expected_users = expected_users();

    let users = Users::typed_get_multiple_users::<_, User>(&pool, params!()).await?;
    println!("{} users inserted. Continuing with some tests:", users.len());

    print!("  * Typed results...      ");
    typed_example(&pool, &expected_users).await?;

    print!("  * Untyped results...    ");
    untyped_example(&pool, &expected_users).await?;

    print!("  * Mapped results...     ");
    mapped_example(&pool, &expected_users).await?;

    print!("  * Conditional blocks... ");
    conditional_example(&pool, &expected_users).await?;

    Users::execute_drop_table(&pool, params!()).await?;
    println!("Dropped users table.");

    Ok(())
}

async fn untyped_example(pool: &SqlitePool, expected: &[User]) -> anyhow::Result<()> {
    let row = Users::untyped_get_user_by_id(pool, params!(1)).await?;
    assert_eq!( User {
        user_id: row.try_get("user_id")?,
        email: row.try_get("email")?,
        name: row.try_get("name")?,
        picture: row.try_get("picture")?,
    }, expected[0]);

    let row = Users::untyped_get_user_by_name(pool, params!("no_such_a_name")).await?;
    assert!(row.is_none());

    let mut it = expected.iter();
    let rows = Users::untyped_get_multiple_users(pool, params!()).await?;

    for row in rows {
        let user = User {
            user_id: row.try_get("user_id")?,
            email: row.try_get("email")?,
            name: row.try_get("name")?,
            picture: row.try_get("picture")?,
        };
        assert_eq!(&user, it.next().unwrap());
    }

    let mut it = expected.iter();
    let mut rows = Users::untyped_get_stream_users(pool, params!()).await;

    while let Some(row) = rows.try_next().await? {
        let user = User {
            user_id: row.try_get("user_id")?,
            email: row.try_get("email")?,
            name: row.try_get("name")?,
            picture: row.try_get("picture")?,
        };
        assert_eq!(&user, it.next().unwrap());
    }
    println!("[OK]");
    Ok(())
}

async fn mapped_example(pool: &SqlitePool, expected: &[User]) -> anyhow::Result<()> {
    let user = Users::mapped_get_user_by_id(pool, params!(1), user_mapper).await?;
    assert_eq!(user, expected[0]);

    let row = Users::mapped_get_user_by_name(pool, params!("Name_Not_exist"), user_mapper).await?;
    assert!(row.is_none());

    let mut it = expected.iter();
    let rows = Users::mapped_get_multiple_users(pool, params!(), user_mapper).await?;
    for user in rows {
        assert_eq!(&user, it.next().unwrap());
    }

    let mut it = expected.iter();
    let mut rows = Users::mapped_get_stream_users(pool, params!(), user_mapper).await;
    while let Some(user) = rows.try_next().await? {
        assert_eq!(&user, it.next().unwrap());
    }
    println!("[OK]");
    Ok(())
}

async fn typed_example(pool: &SqlitePool, expected: &[User]) -> anyhow::Result<()> {
    let user = Users::typed_get_user_by_id::<_, User>(pool, params!(1)).await?;
    assert_eq!(user, expected[0]);

    let row = Users::typed_get_user_by_name::<_, User>(pool, params!("no_such_a_name")).await?;
    assert!(row.is_none());

    let mut it = expected.iter();
    let rows = Users::typed_get_multiple_users::<_, User>(pool, params!()).await?;
    for user in rows {
        assert_eq!(&user, it.next().unwrap());
    }

    let mut it = expected.iter();
    let mut rows = Users::typed_get_stream_users::<_, User>(pool, params!()).await;
    while let Some(got) = rows.try_next().await? {
        assert_eq!(&got, it.next().unwrap());
    }
    println!("[OK]");
    Ok(())
}

async fn conditional_example(pool: &SqlitePool, _expected: &[User]) -> anyhow::Result<()> {
    let got = Users::conditional_query::<_, User>(pool, |cond_id| match cond_id {
        ConditionalQuery::Limited => true
    }, params!()).await?;
    assert_eq!(got.len(), 1);

    println!("[OK]");
    Ok(())
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
#[allow(dead_code)]
pub struct User {
    pub user_id: i32,
    pub email: String,
    pub name: String,
    pub picture: String,
}

pub fn expected_users() -> Vec<User> {
    vec![
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
    ]
}

pub fn sample_data() -> [(i32, &'static str, &'static str, &'static str); 4] {
    [
        (1, "alice@example.com", "Alice", "alice.png"),
        (2, "bob@example.com", "Robert", "robert.png"),
        (3, "charlie@example.com", "Charlie", "charlie.png"),
        (4, "dick@example.com", "Richard", "richard.png"),
    ]
}

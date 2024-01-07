-- :name execute_create_table
CREATE TABLE users
(
  user_id INTEGER PRIMARY KEY,
  email VARCHAR(255) NOT NULL UNIQUE,
  name VARCHAR(255) NOT NULL,
  picture VARCHAR(1024),
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- :name execute_drop_table
DROP TABLE users;

-- :name execute_insert_user
INSERT INTO users(user_id, email, name, picture) VALUES($1, $2, $3, $4);

-- :name untyped_get_user_by_id             :1
SELECT * FROM users WHERE user_id = $1 LIMIT 1;
-- :name untyped_get_user_by_name           :?
SELECT * FROM users WHERE name LIKE $1 LIMIT 1;
-- :name untyped_get_multiple_users         :*
SELECT * FROM users;
-- :name untyped_get_stream_users           :^
SELECT * FROM users;

-- :name typed_get_user_by_id               :typed :1
SELECT * FROM users WHERE user_id = $1 LIMIT 1;
-- :name typed_get_user_by_name             :typed :?
SELECT * FROM users WHERE name LIKE $1 LIMIT 1;
-- :name typed_get_multiple_users           :typed :*
SELECT * FROM users;
-- :name typed_get_stream_users             :typed :^
SELECT * FROM users;

-- :name mapped_get_user_by_id               :mapped :1
SELECT * FROM users WHERE user_id = $1 LIMIT 1;
-- :name mapped_get_user_by_name             :mapped :?
SELECT * FROM users WHERE name LIKE $1 LIMIT 1;
-- :name mapped_get_multiple_users           :mapped :*
SELECT * FROM users;
-- :name mapped_get_stream_users             :mapped :^
SELECT * FROM users;

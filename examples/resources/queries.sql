-- :name add_user
-- :doc Creates new user record
INSERT INTO users(email, name, picture)
VALUES(?, ?, ?)
RETURNING user_id

-- :name fetch_user_by_email :<> :?
-- :doc Returns user based on its identifier
SELECT user_id, email, name, picture
  FROM users
 WHERE email = ?

-- :name add_user :1
-- :doc Creates new user record
INSERT INTO users(email, name, picture)
VALUES(?, ?, ?)
RETURNING user_id, email

-- :name fetch_user_by_email :<> :?
-- :doc Returns user based on its identifier
SELECT user_id, email, name, picture
  FROM users
 WHERE email = ?

-- :name fetch_user_profile :|| :1
-- :doc Returns user's profile
SELECT picture, name
  FROM users
 WHERE user_id = ?
 
-- :name delete_user_by_id
-- :doc Deletes user based on its identifier
DELETE FROM USERS where user_id = ?


# Hug SQLx - embrace SQL

HugSQLx is a derive macro turning SQL queries into plain Rust functions. This is an attempt to decouple queries from source code, embrace IDE's ability to propertly format and syntax-highlight SQLs, and based on underlaying LSP - get auto-completing and docstrings for free.

## Installation

HugSQLx stands on a shoulders of 2 other crates: [async_trait](https://crates.io/crates/async-trait) and [SQLx](https://crates.io/crates/sqlx):

``` toml
[dependencies]
async-trait = "0.1.58"
sqlx = { version = "0.6.2", features = ["sqlite"] }
hugsqlx = { version = "0.1.5", features = ["sqlite"] }
```

Both HugSQLx and SQLx itself should have the same database mentioned in *features* (sqlite, postgres or mysql).

## Deep dive into named queries
The idea here is to distinguish 3 types of queries:
- _typed_ ones - queries which return a result of concrete type, like `<User>`. This is what SQLx returns with `query_as!`.
- _untyped_ ones - queries which return a "raw" database result wrapped into database-specific type (`PgRow`, `SqliteRow` or `MysqlRow`)
- _mapped_ ones - queries where result is transformed by a mapper function rather than coerced with type given upfront. This is what SQLx does by calling `query(..).map(|f| ...)`

Each of these queries (with some exception mentioned below) might return different kind and number of results:
one result, many results, optional result or stream of results. In all cases result might be typed or it might be just a DB row. One exception to this classification a [low-level "execute" query](https://github.com/launchbadge/sqlx#querying) which is always _untyped_ and returns low-level DB-specific result (`PgQueryResult`, `SqliteQueryResult` or `MySqlQueryResult`).

### Query definition
Queries are described by a simple structure:

``` sql
-- :name fetch_users
-- :doc Returns all the users from DB
SELECT user_id, email, name, picture FROM users
```

Crucial part here are 2 lines of comments: one with `:name` identifier, the other one with `:doc` docstring. Note that name needs to be a valid identifier - it's used to generate a function name after all. Use it wisely, no whitechars, hyphens or any other weird characters if you don't want to be surprised by a panic :)

`:doc` on the other hand gives more freedom. Place here anything you'd normally add as a function docstring. In case you'd need multiline docstring, go as following:

``` sql
-- :name set_picture
-- :doc Sets user's picture.
-- Picture is expected to be a valid URL.
UPDATE users
   -- expected URL to the picture
   SET picture = ?
 WHERE user_id = ?
```

This example also shows that it's perfectly valid to use SQL comments inside the query, as long as comment lines do not start with `-- :name` or `-- :doc`, obviously.
### Query type definition
Going along with typed / untyped / mapped classification, here is how to add a type hint to query definition:

``` sql
-- :name untyped_query
-- :name untyped_query :untyped
-- :name typed_query   :typed
-- :name mapped_query  :mapped
```

Queries are *untyped by default*. Nothing's needed to instruct HugSqlx to generated ones (though you may still use `:untyped` hint). The other type however needs a clear hint - either `:typed` one (aliased by `:<>`) for typed query, or `:mapped` (aliased by `:||`) one for mapped query.

### Query result
Again a hint is required to let code generator know what kind of result we expect:

``` sql
-- :name execute
-- :name one_result        :1
-- :name optional_result   :?
-- :name many_results      :*
-- :name stream_of_results :^
```

Analogically to query types, *execute query is default one*. No need for hint here. The other kind of result requires hinting - `:1` when query is expected to return exactly one result, `:?` if optional result is expected, `:*` for many results (vector) and `:^` for a stream of results.

Both query- and result types can be mixed:

``` sql
-- :name delete_user
-- :name fetch_user     :<> :?
-- :name fetch_users    :<> :*
-- :name fetch_profile  :mapped :1
```

## Back to code
When using Hugsqlx, you need to decide first what database (postgres, sqlite or mysql) the code should be generated for:

``` toml
hugsqlx = {version = "0.1.5", features = ["sqlite"]}
```

Having dependency added, time to add a struct:

``` rust
use hugsqlx::{params, HugSqlx};

#[derive(HugSqlx)]
#[queries = "resources/db/queries/users.sql"]
struct Users {}
```

`queries` attribute needs to be either `CARGO_MANIFEST_DIR` (crate's Cargo.toml directory) or workspace relative path and may point to either a single file (query definitions will be taken from this file only) or a directory. The later forces macro to traverse a path and generate corresponding functions upon found files.

Example:

Assuming following query in "resources/db/queries/users.sql":
``` sql
-- :name fetch_users :mapped :*
-- :doc Returns all the users from DB
SELECT user_id, email, name, picture FROM users WHERE role=?
```

HugSqlx generates a trait function `fetch_users`, which might be shaped differently depending on provided query hints. Independently of hints however, all the generated queries require at least 2 arguments - an `Executor` (Pool, PoolConnection or Connection) and query parameters. Mapped query, as expected, requires one more parameter - a mapper function transforming DB row into a data of concrete type. Let's call the generated function for above query:

``` rust
let users = Users::fetch_users(&pool, params!("guest"), |row| { ... }).await?;
```

Parameters need to be passed with `params!` macro due to Rust mechanism which forbids creating a vector of elements of different types.

## Tips & tricks (with Emacs)
### How to get better syntax highlighting on comments with `:name` and `:doc`?

``` emacs-lisp
(font-lock-add-keywords
 'sql-mode `(("\".+?\"" 0 'font-lock-string-face t)
             (":[a-zA-Z0-9+-><?!\\*\\|]?+" 0 'font-lock-constant-face t)
             (":name \\([[:graph:]]+\\)" 1 'font-lock-function-name-face t)))
```

### How to get get ctags working with named queries?

```
--kinddef-sql=q,query,Queries
--regex-sql=/\-\-[ \t]+(:name[\ \t]+)([[:alnum:]_-]+)/\2/q/
```

## Limitations
Query definition both with `:name` and `:doc` expects `:name` comment to appear first. HugSqlx does not complain otherwise, but result might be surprising.

No subfolders are recursively traversed to read query definitions.

Also, because of SQLx limitation, no named parameters have been implemented yet.

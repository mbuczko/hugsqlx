extern crate proc_macro;

mod parser;

use parser::{Kind, Method, Query};
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use std::{env, fs, path::Path};
use syn::{parse_str, Lit, Meta, MetaNameValue, Type};

pub struct Context(Type, Type, Type, Type);
pub enum ContextType {
    Postgres,
    Sqlite,
    Mysql,
    Default,
}
impl Context {
    pub fn new(context_type: ContextType) -> Self {
        match context_type {
            ContextType::Postgres => Context(
                parse_str::<Type>("sqlx::postgres::Postgres").unwrap(),
                parse_str::<Type>("sqlx::postgres::PgArguments").unwrap(),
                parse_str::<Type>("sqlx::postgres::PgRow").unwrap(),
                parse_str::<Type>("sqlx::postgres::PgQueryResult").unwrap(),
            ),
            ContextType::Sqlite => Context(
                parse_str::<Type>("sqlx::sqlite::Sqlite").unwrap(),
                parse_str::<Type>("sqlx::sqlite::SqliteArguments<'async_trait>").unwrap(),
                parse_str::<Type>("sqlx::sqlite::SqliteRow").unwrap(),
                parse_str::<Type>("sqlx::sqlite::SqliteQueryResult").unwrap(),
            ),
            ContextType::Mysql => Context(
                parse_str::<Type>("sqlx::mysql::MySql").unwrap(),
                parse_str::<Type>("sqlx::mysql::MySqlArguments").unwrap(),
                parse_str::<Type>("sqlx::mysql::MySqlRow").unwrap(),
                parse_str::<Type>("sqlx::mysql::MySqlQueryResult").unwrap(),
            ),
            _ => panic!("None of [postgres, sqlite, mysql] feature enabled"),
        }
    }
}

/// Find all pairs of the `name = "value"` attribute from the derive input
fn find_attribute_values(ast: &syn::DeriveInput, attr_name: &str) -> Vec<String> {
    ast.attrs
        .iter()
        .filter(|value| value.path.is_ident(attr_name))
        .filter_map(|attr| attr.parse_meta().ok())
        .filter_map(|meta| match meta {
            Meta::NameValue(MetaNameValue {
                lit: Lit::Str(val), ..
            }) => Some(val.value()),
            _ => None,
        })
        .collect()
}

pub fn impl_hug_sqlx(ast: &syn::DeriveInput, ctx: Context) -> TokenStream2 {
    let mut queries_paths = find_attribute_values(ast, "queries");
    if queries_paths.len() != 1 {
        panic!(
            "#[derive(HugSql)] must contain one attribute like this #[queries = \"db/queries/\"]"
        );
    }

    let folder_path = queries_paths.remove(0);
    let canonical_path = Path::new(&folder_path)
        .canonicalize()
        .unwrap_or_else(|err| panic!("folder path must resolve to an absolute path: {}", err));

    let cargo_dir = env::var("CARGO_MANIFEST_DIR").expect("Could not locate Cargo.toml");
    let cargo_dir_canonical_path = Path::new(&cargo_dir)
        .canonicalize()
        .unwrap_or_else(|err| panic!("cargo dir path must resolve to an absolute path: {}", err));
    if !canonical_path.starts_with(&cargo_dir_canonical_path) {
        panic!(
            "Queries path must be relative to Cargo.toml location ({})",
            cargo_dir_canonical_path.display(),
        );
    }

    let files = if canonical_path.is_dir() {
        walkdir::WalkDir::new(canonical_path)
            .follow_links(true)
            .sort_by_file_name()
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(move |e| std::fs::canonicalize(e.path()).expect("Could not get canonical path"))
            .collect()
    } else {
        vec![canonical_path]
    };

    let name = &ast.ident;
    let mut output_ts = TokenStream2::new();
    let mut functions = TokenStream2::new();

    for f in files {
        if let Ok(input) = fs::read_to_string(f) {
            match parser::parse_queries(input) {
                Ok(ast) => {
                    generate_impl_fns(ast, &ctx, &mut functions);
                }
                Err(parse_errs) => parse_errs
                    .into_iter()
                    .for_each(|e| eprintln!("Parse error: {}", e)),
            }
        }
    }

    output_ts.extend(quote! {
        #[async_trait::async_trait]
        pub trait HugSql<'q> {
            #functions
        }
        impl<'q> HugSql<'q> for #name {
        }
    });
    output_ts
}

fn generate_impl_fns(queries: Vec<Query>, ctx: &Context, output_ts: &mut TokenStream2) {
    for q in queries {
        if let Some(doc) = &q.doc {
            output_ts.extend(quote! { #[doc = #doc] });
        }
        match q.kind {
            Kind::Typed => generate_typed_fn(q, ctx, output_ts),
            Kind::Untyped => generate_untyped_fn(q, ctx, output_ts),
            Kind::Mapped => generate_mapped_fn(q, ctx, output_ts),
        }
    }
}

fn generate_typed_fn(
    q: Query,
    Context(db, args, row, result): &Context,
    output_ts: &mut TokenStream2,
) {
    let name = format_ident!("{}", q.name);
    let sql = q.sql;

    output_ts.extend(match q.method {
        Method::FetchMany => {
            quote! {
                async fn #name<'e, E, T> (conn: E, params: #args) -> futures_core::stream::BoxStream<'e, Result<T, sqlx::Error>>
                where E: sqlx::Executor<'e, Database = #db>,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch(conn)
                }
            }
        },
        Method::FetchOne => {
            quote! {
                async fn #name<'e, E, T> (conn: E, params: #args) -> Result<T, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db>,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch_one(conn).await
                }
            }
        },
        Method::FetchOptional => {
            quote! {
                async fn #name<'e, E, T> (conn: E, params: #args) -> Result<Option<T>, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db>,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch_optional(conn).await
                }
            }
        },
        Method::FetchAll => {
            quote! {
                async fn #name<'e, E, T> (conn: E, params: #args) -> Result<Vec<T>, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db>,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch_all(conn).await
                }
            }
        },
        Method::Execute => {
            quote! {
                async fn #name<'e, E> (conn: E, params: #args) -> Result<#result, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).execute(conn).await
                }
            }
        },
    });
}

fn generate_untyped_fn(
    q: Query,
    Context(db, args, row, result): &Context,
    output_ts: &mut TokenStream2,
) {
    let name = format_ident!("{}", q.name);
    let sql = q.sql;

    output_ts.extend(match q.method {
        Method::FetchMany => {
            quote! {
                async fn #name<'e, E> (conn: E, params: #args) -> futures_core::stream::BoxStream<'e, Result<#row, sqlx::Error>>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).fetch(conn)
                }
            }
        },
        Method::FetchOne => {
            quote! {
                async fn #name<'e, E> (conn: E, params: #args) -> Result<#row, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).fetch_one(conn).await
                }
            }
        },
        Method::FetchOptional => {
            quote! {
                async fn #name<'e, E> (conn: E, params: #args) -> Result<Option<#row>, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).fetch_optional(conn).await
                }
            }
        },
        Method::FetchAll => {
            quote! {
                async fn #name<'e, E> (conn: E, params: #args) -> Result<Vec<#row>, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).fetch_all(conn).await
                }
            }
        },
        Method::Execute => {
            quote! {
                async fn #name<'e, E> (conn: E, params: #args) -> Result<#result, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).execute(conn).await
                }
            }
        },
    });
}

fn generate_mapped_fn(
    q: Query,
    Context(db, args, row, result): &Context,
    output_ts: &mut TokenStream2,
) {
    let name = format_ident!("{}", q.name);
    let sql = q.sql;

    output_ts.extend(match q.method {
        Method::FetchMany => {
            quote! {
                async fn #name<'e, E, F, T> (conn: E, params: #args, mut mapper: F) -> futures_core::stream::BoxStream<'e, Result<T, sqlx::Error>>
                where E: sqlx::Executor<'e, Database = #db>,
                      F: FnMut(#row) -> T + Send,
                      T: Send + Unpin {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch(conn)
                }
            }
        },
        Method::FetchOne => {
            quote! {
                async fn #name<'e, E, F, T> (conn: E, params: #args, mut mapper: F) -> Result<T, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db>,
                      F: FnMut(#row) -> T + Send,
                      T: Send + Unpin {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch_one(conn)
                        .await
                }
            }
        },
        Method::FetchOptional => {
            quote! {
                async fn #name<'e, E, F, T> (conn: E, params: #args, mut mapper: F) -> Result<Option<T>, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db>,
                      F: FnMut(#row) -> T + Send,
                      T: Send + Unpin {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch_optional(conn)
                        .await
                }
            }
        },
        Method::FetchAll => {
            quote! {
                async fn #name<'e, E, F, T> (conn: E, params: #args, mut mapper: F) -> Result<Vec<T>, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db>,
                      F: FnMut(#row) -> T + Send,
                      T: Send + Unpin {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch_all(conn)
                        .await
                }
            }
        },
        Method::Execute => {
            quote! {
                async fn #name<'e, E, F, T> (conn: E, params: #args) -> Result<#result, sqlx::Error>
                where E: sqlx::Executor<'e, Database = #db> {
                    sqlx::query_with(#sql, params).execute(conn).await
                }
            }
        },
    });
}

cfg_if::cfg_if! {
    if #[cfg(feature = "postgres")] {
        #[macro_export]
        macro_rules! params {
            ($($arg:expr),*) => {
                {
                    use sqlx::Arguments;
                    let mut args = sqlx::postgres::PgArguments::default();
                    $( args.add($arg); )*
                    args
                }
            };
        }
    } else if #[cfg(feature = "mysql")] {
        #[macro_export]
        macro_rules! params {
            ($($arg:expr),*) => {
                {
                    use sqlx::Arguments;
                    let mut args = sqlx::mysql::MySqlArguments::default();
                    $( args.add($arg); )*
                    args
                }
            };
        }
    } else {
        #[macro_export]
        macro_rules! params {
            ($($arg:expr),*) => {
                {
                    use sqlx::Arguments;
                    let mut args = sqlx::sqlite::SqliteArguments::default();
                    $( args.add($arg); )*
                    args
                }
            };
        }
    }
}

#[cfg(test)]
mod test {
    use crate::parser::{query_parser, Kind, Method};
    use chumsky::Parser;

    #[test]
    fn parsing_defaults() {
        let input = r#"
-- :name fetch_users
-- :doc Returns all the users from DB
SELECT user_id, email, name, picture FROM users
"#;

        let queries = query_parser().parse(input).unwrap();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].name, "fetch_users");
        assert_eq!(
            queries[0].doc,
            Some("Returns all the users from DB".to_string())
        );
        assert_eq!(queries[0].kind, Kind::Untyped);
        assert_eq!(queries[0].method, Method::Execute);
    }

    #[test]
    fn parsing_default_type() {
        let input = r#"
-- :name fetch_users :^
SELECT user_id, email, name, picture FROM users
"#;

        let queries = query_parser().parse(input).unwrap();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].name, "fetch_users");
        assert_eq!(queries[0].doc, None);
        assert_eq!(queries[0].kind, Kind::Untyped);
        assert_eq!(queries[0].method, Method::FetchMany);
    }

    #[test]
    fn parsing_type_aliases() {
        let input = r#"
-- :name fetch_users :<> :^
SELECT user_id, email, name, picture FROM users
"#;

        let queries = query_parser().parse(input).unwrap();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].name, "fetch_users");
        assert_eq!(queries[0].doc, None);
        assert_eq!(queries[0].kind, Kind::Typed);
        assert_eq!(queries[0].method, Method::FetchMany);
    }

    #[test]
    fn parsing_default_call_method() {
        let input = r#"
-- :name fetch_users :mapped
SELECT user_id, email, name, picture FROM users
"#;

        let queries = query_parser().parse(input).unwrap();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].name, "fetch_users");
        assert_eq!(queries[0].doc, None);
        assert_eq!(queries[0].kind, Kind::Mapped);
        assert_eq!(queries[0].method, Method::Execute);
    }

    #[test]
    fn parsing_multiple() {
        let input = r#"
-- :name fetch_users
-- :doc Returns all the users from DB
SELECT user_id, email, name, picture FROM users

-- :name fetch_user_by_id :untyped :1
-- :doc Fetches user by its identifier
SELECT user_id, email, name, picture
  FROM users
 WHERE user_id = $1

-- :name set_picture :typed :1
-- :doc Sets user's picture.
-- Picture is expected to be a valid URL.
UPDATE users
   -- expected URL to the picture
   SET picture = ?
 WHERE user_id = ?

-- :name delete_user :typed :1
DELETE FROM users
 WHERE user_id = ?
"#;

        let queries = query_parser().parse(input).unwrap();
        assert_eq!(queries.len(), 4);

        assert_eq!(queries[0].name, "fetch_users".to_string());
        assert_eq!(
            queries[0].doc,
            Some("Returns all the users from DB".to_string())
        );
        assert_eq!(queries[0].kind, Kind::Untyped);
        assert_eq!(queries[0].method, Method::Execute);

        assert_eq!(queries[1].name, "fetch_user_by_id".to_string());
        assert_eq!(
            queries[1].doc,
            Some("Fetches user by its identifier".to_string())
        );
        assert_eq!(queries[1].kind, Kind::Untyped);
        assert_eq!(queries[1].method, Method::FetchOne);

        assert_eq!(queries[2].name, "set_picture".to_string());
        assert_eq!(
            queries[2].doc,
            Some("Sets user's picture.\nPicture is expected to be a valid URL.".to_string())
        );
        assert_eq!(queries[2].kind, Kind::Typed);
        assert_eq!(queries[2].method, Method::FetchOne);

        assert_eq!(queries[3].name, "delete_user".to_string());
        assert_eq!(queries[3].doc, None);
        assert_eq!(queries[3].kind, Kind::Typed);
        assert_eq!(queries[3].method, Method::FetchOne);
    }
}

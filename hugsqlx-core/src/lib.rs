extern crate proc_macro;

mod condblock;
mod parser;

use parser::{Kind, Method, Query};
use proc_macro2::{Ident, Span, TokenStream as TokenStream2};
use quote::quote;
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};
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
                parse_str::<Type>("sqlx::sqlite::SqliteArguments<'q>").unwrap(),
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

fn workspace_dir() -> PathBuf {
    let output = std::process::Command::new(env!("CARGO"))
        .arg("locate-project")
        .arg("--workspace")
        .arg("--message-format=plain")
        .output()
        .unwrap()
        .stdout;
    let cargo_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
    cargo_path
        .parent()
        .unwrap()
        .to_path_buf()
        .canonicalize()
        .unwrap_or_else(|err| {
            panic!(
                "workspace dir path must resolve to an absolute path: {}",
                err
            )
        })
}

fn snake_to_pascal(snake: &str) -> String {
    let mut result = String::with_capacity(snake.len());
    let mut capitalize_next = true;

    for ch in snake.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

/// Find a suitable candidate queries path by both the local crate's CARGO_MANIFEST_DIR
/// as well as the workspace root.
pub fn find_queries_path(queries_path: String) -> PathBuf {
    // The directory of the crate's cargo dir. This may be different from the workspace root's directory.
    let cargo_dir = env::var("CARGO_MANIFEST_DIR").expect("Could not locate Cargo.toml");
    let cargo_dir_canonical_path = Path::new(&cargo_dir)
        .canonicalize()
        .unwrap_or_else(|err| panic!("cargo dir path must resolve to an absolute path: {}", err));

    let mut seen = BTreeSet::new();
    let candidate_path = cargo_dir_canonical_path.join(&queries_path);
    if candidate_path.exists() {
        return candidate_path;
    }
    seen.insert(cargo_dir_canonical_path);

    let workspace_root = workspace_dir();
    let candidate_path = workspace_root.join(&queries_path);

    if candidate_path.exists() {
        return candidate_path;
    }

    seen.insert(workspace_root);
    panic!("Queries path must be relative to the crate's Cargo.toml location or the workspace root. Tried the following folders: {seen:?}");
}

pub fn impl_hug_sqlx(ast: &syn::DeriveInput, ctx: Context) -> TokenStream2 {
    let mut queries_paths = find_attribute_values(ast, "queries");
    if queries_paths.len() != 1 {
        panic!(
            "#[derive(HugSql)] must contain one attribute like this #[queries = \"db/queries/\"]"
        );
    }
    let canonical_path = find_queries_path(queries_paths.remove(0));

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
    let mut enums = TokenStream2::new();

    for f in files {
        if let Ok(input) = fs::read_to_string(f) {
            match parser::parse_queries(input) {
                Ok(ast) => {
                    generate_impl_fns(ast, &ctx, &mut functions, &mut enums);
                }
                Err(parse_errs) => parse_errs
                    .into_iter()
                    .for_each(|e| eprintln!("Parse error: {}", e)),
            }
        }
    }

    output_ts.extend(quote! {
        #enums

        pub trait HugSql {
            #functions
        }
        impl HugSql for #name {
        }
    });
    output_ts
}

fn generate_cond_block_resolver_fn(query: &Query) -> (TokenStream2, TokenStream2, TokenStream2) {
    let sql_blocks = &query.sql;
    let cond_blocks = sql_blocks
        .iter()
        .filter(|b| matches!(b, condblock::SqlBlock::Conditional(_, _)))
        .count();

    if cond_blocks > 0 {
        let enumeration = Ident::new(&snake_to_pascal(&query.name), Span::call_site());
        let mut variants = Vec::with_capacity(cond_blocks);

        // Generate compile-time code that builds the SQL string at runtime
        let block_processing: Vec<_> = sql_blocks
            .iter()
            .map(|block| match block {
                condblock::SqlBlock::Conditional(id, sql) => {
                    let variant = Ident::new(&snake_to_pascal(id), Span::call_site());
                    let quot = quote! {
                        if block_resolver(#enumeration::#variant) {
                            result.push('\n');
                            result.push_str(#sql);
                        }
                    };
                    variants.push(variant);
                    quot
                }
                condblock::SqlBlock::Literal(sql) => {
                    quote! {
                        result.push_str(#sql);
                    }
                }
            })
            .collect();

        // Generate Enums that will be passed to block resolving function
        let variant_tokens = variants.into_iter().map(|variant| {
            quote! {
                #variant,
            }
        });

        return (
            quote! { block_resolver: impl Fn(#enumeration) -> bool + Send, },
            quote! {
                &{
                    let mut result = String::new();
                    #(#block_processing)*
                    result
                }
            },
            quote! {
                pub enum #enumeration {
                    #(#variant_tokens)*
                }
            },
        );
    }
    let sql = match sql_blocks.first() {
        Some(condblock::SqlBlock::Literal(sql))
        | Some(condblock::SqlBlock::Conditional(_, sql)) => sql,
        None => "",
    };
    (TokenStream2::new(), quote! { #sql }, TokenStream2::new())
}

fn generate_impl_fns(
    queries: Vec<Query>,
    ctx: &Context,
    functions_ts: &mut TokenStream2,
    enums_ts: &mut TokenStream2,
) {
    for q in queries {
        if let Some(doc) = &q.doc {
            functions_ts.extend(quote! { #[doc = #doc] });
        }
        match q.kind {
            Kind::Typed => generate_typed_fn(q, ctx, functions_ts, enums_ts),
            Kind::Untyped => generate_untyped_fn(q, ctx, functions_ts, enums_ts),
            Kind::Mapped => generate_mapped_fn(q, ctx, functions_ts, enums_ts),
        }
    }
}

fn generate_typed_fn(
    q: Query,
    Context(db, args, row, result): &Context,
    functions_ts: &mut TokenStream2,
    enums_ts: &mut TokenStream2,
) {
    let name = Ident::new(&q.name, Span::call_site());
    let (block_resolver, sql, enums) = generate_cond_block_resolver_fn(&q);

    enums_ts.extend(enums);

    functions_ts.extend(match q.method {
        Method::FetchMany => {
            quote! {
                async fn #name<'q, 'e, 'c, E, T> (executor: E, #block_resolver params: #args) -> futures_core::stream::BoxStream<'e, Result<T, sqlx::Error>>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch(executor)
                }
            }
        },
        Method::FetchOne => {
            quote! {
                async fn #name<'q, 'e, 'c, E, T> (executor: E, #block_resolver params: #args) -> Result<T, sqlx::Error>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch_one(executor).await
                }
            }
        },
        Method::FetchOptional => {
            quote! {
                async fn #name<'q, 'e, 'c, E, T> (executor: E, #block_resolver params: #args) -> Result<Option<T>, sqlx::Error>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch_optional(executor).await
                }
            }
        },
        Method::FetchAll => {
            quote! {
                async fn #name<'q, 'e, 'c, E, T> (executor: E, #block_resolver params: #args) -> Result<Vec<T>, sqlx::Error>
                where
                     'q: 'e,
                     'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      T: Send + Unpin + for<'r> sqlx::FromRow<'r, #row> + 'e {
                    sqlx::query_as_with(#sql, params).fetch_all(executor).await
                }
            }
        },
        Method::Execute => {
            quote! {
                async fn #name<'q, 'e, 'c, E> (executor: E, #block_resolver params: #args) -> Result<#result, sqlx::Error>
                where
                 'q: 'e,
                 'c: 'e,
                 E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).execute(executor).await
                }
            }
        },
    });
}

fn generate_untyped_fn(
    q: Query,
    Context(db, args, row, result): &Context,
    functions_ts: &mut TokenStream2,
    enums_ts: &mut TokenStream2,
) {
    let name = Ident::new(&q.name, Span::call_site());
    let (block_resolver, sql, enums) = generate_cond_block_resolver_fn(&q);

    enums_ts.extend(enums);

    functions_ts.extend(match q.method {
        Method::FetchMany => {
            quote! {
                async fn #name<'q, 'e, 'c, E> (executor: E, #block_resolver params: #args) -> futures_core::stream::BoxStream<'e, Result<#row, sqlx::Error>>
                where
                 'q: 'e,
                 'c: 'e,
                 E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).fetch(executor)
                }
            }
        },
        Method::FetchOne => {
            quote! {
                async fn #name<'q, 'e, 'c, E> (executor: E, #block_resolver params: #args) -> Result<#row, sqlx::Error>
                where
                 'q: 'e,
                 'c: 'e,
                 E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).fetch_one(executor).await
                }
            }
        },
        Method::FetchOptional => {
            quote! {
                async fn #name<'q, 'e, 'c, E> (executor: E, #block_resolver params: #args) -> Result<Option<#row>, sqlx::Error>
                where
                 'q: 'e,
                 'c: 'e,
                 E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).fetch_optional(executor).await
                }
            }
        },
        Method::FetchAll => {
            quote! {
                async fn #name<'q, 'e, 'c, E> (executor: E, #block_resolver params: #args) -> Result<Vec<#row>, sqlx::Error>
                where
                 'q: 'e,
                 'c: 'e,
                 E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).fetch_all(executor).await
                }
            }
        },
        Method::Execute => {
            quote! {
                async fn #name<'q, 'e, 'c, E> (executor: E, #block_resolver params: #args) -> Result<#result, sqlx::Error>
                where
                 'q: 'e,
                 'c: 'e,
                 E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).execute(executor).await
                }
            }
        },
    });
}

fn generate_mapped_fn(
    q: Query,
    Context(db, args, row, result): &Context,
    functions_ts: &mut TokenStream2,
    enums_ts: &mut TokenStream2,
) {
    let name = Ident::new(&q.name, Span::call_site());
    let (block_resolver, sql, enums) = generate_cond_block_resolver_fn(&q);

    enums_ts.extend(enums);

    functions_ts.extend(match q.method {
        Method::FetchMany => {
            quote! {
                async fn #name<'q, 'e, 'c, E, F, T> (executor: E, #block_resolver params: #args, mapper: F) -> futures_core::stream::BoxStream<'e, Result<T, sqlx::Error>>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      F: FnMut(#row) -> T + Send + 'e,
                      T: Send + Unpin + 'e {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch(executor)
                }
            }
        },
        Method::FetchOne => {
            quote! {
                async fn #name<'q, 'e, 'c, E, F, T> (executor: E, #block_resolver params: #args, mapper: F) -> Result<T, sqlx::Error>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      F: FnMut(#row) -> T + Send + 'e,
                      T: Send + Unpin + 'e {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch_one(executor)
                        .await
                }
            }
        },
        Method::FetchOptional => {
            quote! {
                async fn #name<'q, 'e, 'c, E, F, T> (executor: E, #block_resolver params: #args, mapper: F) -> Result<Option<T>, sqlx::Error>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      F: FnMut(#row) -> T + Send + 'e,
                      T: Send + Unpin + 'e {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch_optional(executor)
                        .await
                }
            }
        },
        Method::FetchAll => {
            quote! {
                async fn #name<'q, 'e, 'c, E, F, T> (executor: E, #block_resolver params: #args, mapper: F) -> Result<Vec<T>, sqlx::Error>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e,
                      F: FnMut(#row) -> T + Send + 'e,
                      T: Send + Unpin + 'e {
                    sqlx::query_with(#sql, params)
                        .map(mapper)
                        .fetch_all(executor)
                        .await
                }
            }
        },
        Method::Execute => {
            quote! {
                async fn #name<'q, 'e, 'c, E, F, T> (executor: E, #block_resolver params: #args) -> Result<#result, sqlx::Error>
                where
                      'q: 'e,
                      'c: 'e,
                      E: sqlx::Executor<'c, Database = #db> + 'e {
                    sqlx::query_with(#sql, params).execute(executor).await
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
                    $( args.add($arg).unwrap(); )*
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
                    $( args.add($arg).unwrap(); )*
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
                    $( args.add($arg).unwrap(); )*
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

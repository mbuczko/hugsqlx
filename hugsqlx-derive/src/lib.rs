use hugsqlx_core::{Context, ContextType};
use proc_macro::TokenStream;

#[proc_macro_derive(HugSqlx, attributes(queries))]
pub fn hugsqlx(input_stream: TokenStream) -> TokenStream {
    let ast = syn::parse(input_stream).unwrap();
    let ctx = Context::new(if cfg!(feature = "postgres") {
        ContextType::Postgres
    } else if cfg!(feature = "sqlite") {
        ContextType::Sqlite
    } else if cfg!(feature = "mysql") {
        ContextType::Mysql
    } else {
        ContextType::Default
    });
    hugsqlx_core::impl_hug_sqlx(&ast, ctx).into()
}

use hugsqlx_core::{Context, ContextDb};
use proc_macro::TokenStream;

#[proc_macro_derive(HugSqlx, attributes(queries))]
pub fn hugsqlx(input_stream: TokenStream) -> TokenStream {
    let ast = syn::parse(input_stream).unwrap();
    let ctx = Context::new(if cfg!(feature = "postgres") {
        ContextDb::Postgres
    } else if cfg!(feature = "sqlite") {
        ContextDb::Sqlite
    } else if cfg!(feature = "mysql") {
        ContextDb::Mysql
    } else {
        ContextDb::Default
    });

    hugsqlx_core::impl_hug_sqlx(&ast, ctx).into()
}

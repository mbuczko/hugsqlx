use chumsky::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Typed,
    Untyped,
    Mapped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Method {
    FetchAll,
    FetchOne,
    FetchOptional,
    FetchMany,
    Execute,
}

#[derive(Debug, PartialEq, Eq)]
enum Element {
    Signature(String, Kind, Method),
    Doc(String),
    Sql(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub name: String,
    pub kind: Kind,
    pub method: Method,
    pub doc: Option<String>,
    pub sql: String,
}

impl Query {
    fn from(elements: Vec<Element>) -> Self {
        let mut name = String::default();
        let mut doc = None;
        let mut sql = String::default();
        let mut kind = Kind::Typed;
        let mut method = Method::FetchAll;

        for e in elements {
            match e {
                Element::Signature(n, t, m) => {
                    name = n;
                    kind = t;
                    method = m;
                }
                Element::Doc(d) => doc = Some(d),
                Element::Sql(s) => sql = s,
            }
        }
        if name.is_empty() {
            panic!(
                ":name attribute is missing or is not a valid identifier. Query: \"{}\"",
                sql.trim()
            );
        }
        Query {
            name,
            kind,
            method,
            doc,
            sql,
        }
    }
}

pub(crate) fn query_parser() -> impl Parser<char, Vec<Query>, Error = Simple<char>> {
    let comment = just("--").padded();

    let arity = just(':')
        .ignore_then(choice((
            just('!').to(Method::Execute),
            just('1').to(Method::FetchOne),
            just('?').to(Method::FetchOptional),
            just('*').to(Method::FetchAll),
            just('^').to(Method::FetchMany),
        )))
        .padded()
        .labelled("arity");

    let kind = just(':')
        .ignore_then(choice((
            just("<>").to(Kind::Typed),
            just("||").to(Kind::Mapped),
            text::keyword("typed").to(Kind::Typed),
            text::keyword("mapped").to(Kind::Mapped),
            text::keyword("untyped").to(Kind::Untyped),
        )))
        .padded()
        .labelled("type");

    let signature = comment
        .ignore_then(just(':'))
        .ignore_then(just("name").padded())
        .ignore_then(text::ident())
        .padded()
        .then(kind.or_not().then(arity.or_not()))
        .map(|(ident, (t, a))| {
            Element::Signature(
                ident,
                t.unwrap_or(Kind::Untyped),
                a.unwrap_or(Method::Execute),
            )
        })
        .labelled("name");

    let doc = comment
        .ignore_then(just(':'))
        .ignore_then(just("doc").padded())
        .ignore_then(take_until(just('\n')))
        .then(
            comment
                .ignore_then(take_until(just('\n')))
                .padded()
                .repeated(),
        )
        .foldl(|(mut v, c), rhs| {
            v.push(c);
            v.extend(rhs.0);
            (v, c)
        })
        .map(|(v, _)| Element::Doc(v.iter().collect::<String>()))
        .labelled("doc");

    let signature_cloned = signature.clone();
    let sql = take_until(signature_cloned.or(doc).rewind().ignored().or(end()))
        .padded()
        .map(|(v, _)| Element::Sql(v.iter().collect::<String>()))
        .labelled("sql");

    let query = signature
        .or(doc)
        .repeated()
        .at_least(1)
        .at_most(2)
        .chain(sql)
        .map(Query::from);

    query.repeated().then_ignore(end())
}

pub fn parse_queries(input: String) -> Result<Vec<Query>, Vec<Simple<char>>> {
    query_parser().parse(input)
}

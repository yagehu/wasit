use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "witx.pest"]
pub struct WitxParser;

#[cfg(test)]
mod tests {
    use pest::Parser;

    use super::*;

    #[test]
    fn ok() {
        const DOC: &str = include_str!("../main.witx");

        let pairs = match WitxParser::parse(Rule::document, DOC) {
            Ok(mut pairs) => pairs.next().unwrap(),
            Err(err) => {
                eprintln!("{}", err);
                panic!()
            },
        };

        eprintln!("{:#?}", pairs);
    }
}

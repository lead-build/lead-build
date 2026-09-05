#[derive(Debug, PartialEq)]
pub enum StringType {
    Str(String),
    Expr(String),
}

pub fn string_decode(input: &str) -> Option<Vec<StringType>> {
    let mut input = input.chars();

    let mut output: Vec<StringType> = Vec::new();
    let mut buf = String::new();

    loop {
        match input.next() {
            Some('$') => match input.next() {
                Some('{') => {
                    if !buf.is_empty() {
                        output.push(StringType::Str(buf));
                        buf = String::new();
                    }
                    let mut lvl = 1i32;
                    while lvl > 0 {
                        let c = input.next();
                        if let Some(c) = c {
                            buf.push(c);
                        }
                        match c {
                            Some('{') => lvl += 1,
                            Some('}') => lvl -= 1,
                            _ => {}
                        }
                    }
                    buf.pop();
                    output.push(StringType::Expr(buf));
                    buf = String::new();
                }
                Some('$') => buf.push('$'),
                _ => return None,
            },
            Some(c) => buf.push(c),
            None => break,
        }
    }
    if !buf.is_empty() {
        output.push(StringType::Str(buf));
    }

    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_empty() {
        assert_eq!(string_decode(""), Some(vec![]))
    }

    #[test]
    fn decode_simple() {
        assert_eq!(
            string_decode("test"),
            Some(vec![StringType::Str("test".into())])
        )
    }

    #[test]
    fn decode_expr() {
        assert_eq!(
            string_decode("${code}"),
            Some(vec![StringType::Expr("code".into())])
        )
    }

    #[test]
    fn decode_str_code_str() {
        assert_eq!(
            string_decode("abc${def}ghi"),
            Some(vec![
                StringType::Str("abc".into()),
                StringType::Expr("def".into()),
                StringType::Str("ghi".into()),
            ])
        )
    }

    #[test]
    fn decode_code_code() {
        assert_eq!(
            string_decode("${abc}${def}"),
            Some(vec![
                StringType::Expr("abc".into()),
                StringType::Expr("def".into()),
            ])
        )
    }
}

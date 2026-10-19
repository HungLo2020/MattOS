// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::details::property::syntax::parse_syntax;
use crate::details::property::value::parse_values;
use crate::parseerror::{ParseErrorKind, SourceLocation};
use crate::value::{Color, Dimension, Value, Unit};

fn check_value(input: (&str, &str), expected: Vec<Value>) {
    let mut parser_input = cssparser::ParserInput::new(input.1);
    let mut parser = cssparser::Parser::new(&mut parser_input);

    let parsed_syntax = parse_syntax(input.0, SourceLocation::from_file("Test Input")).unwrap();

    let result = parse_values(&parsed_syntax, &mut parser);
    match result {
        Ok(values) => assert_eq!(values, expected),
        Err(error) => panic!("{}", error),
    }
}

test_cases! {
    length_px:
        check_value ("<length>", "24px"), vec![
            Value::from(Dimension::px(24.0))
        ];
    length_em:
        check_value ("<length>", "3em"), vec![
            Value::from(Dimension{value: 3.0, unit: Unit::Em})
        ];
    length_list:
        check_value ("<length>+", "1px 2px 3px"), vec![
            Value::from(Dimension::px(1.0)),
            Value::from(Dimension::px(2.0)),
            Value::from(Dimension::px(3.0)),
        ];
    number:
        check_value ("<number>", "66.6"), vec![
            Value::from(66.6)
        ];
    color_hex:
        check_value ("<color>", "#ff0000"), vec![
            Value::from(Color::rgba(255, 0, 0, 255))
        ];
    color_hex_short:
        check_value ("<color>", "#0f0"), vec![
            Value::from(Color::rgba(0, 255, 0, 255))
        ];
    color_named:
        check_value ("<color>", "blue"), vec![
            Value::from(Color::rgba(0, 0, 255, 255))
        ];
    color_comma_list:
        check_value ("<color>#", "red, green, blue"), vec![
            Value::from(Color::rgba(255, 0, 0, 255)),
            Value::from(Color::rgba(0, 128, 0, 255)),
            Value::from(Color::rgba(0, 0, 255, 255)),
        ];
    universal:
        check_value ("*", "#ff0000"), vec![
            Value::from(Color::rgba(255, 0, 0, 255))
        ];
    alternative_keyword:
        check_value ("auto | <length>", "auto"), vec![
            Value::from("auto")
        ];
    alternative_value:
        check_value ("auto | <length>", "24px"), vec![
            Value::from(Dimension::px(24.0))
        ];
    group_keyword:
        check_value ("(auto | <number>) | (<length> <length>)", "auto"), vec![
            Value::from("auto"),
        ];
    group_number:
        check_value ("(auto | <number>) | (<length> <length>)", "24.0"), vec![
            Value::from(24.0),
        ];
    group_lengths:
        check_value ("(auto | <number>) | (<length> <length>)", "24px 24px"), vec![
            Value::from(Dimension::px(24.0)),
            Value::from(Dimension::px(24.0)),
        ];
    repeat:
        check_value ("<angle>{2,4}", "90deg 180deg 270deg"), vec![
            Value::from(Dimension{value: 90.0, unit: Unit::Degrees}),
            Value::from(Dimension{value: 180.0, unit: Unit::Degrees}),
            Value::from(Dimension{value: 270.0, unit: Unit::Degrees}),
        ];
}

fn check_error(syntax: &str, input: &str) {
    let mut parser_input = cssparser::ParserInput::new(input);
    let mut parser = cssparser::Parser::new(&mut parser_input);

    let parsed_syntax = parse_syntax(syntax, SourceLocation::from_file("Test Input")).unwrap();

    let result = parse_values(&parsed_syntax, &mut parser);
    match result {
        Ok(values) => panic!("Expected error, got Ok({:?})", values),
        Err(error) => {
            if let cssparser::ParseErrorKind::Custom(parse_error) = error.kind {
                assert_eq!(parse_error.kind, ParseErrorKind::PropertyValueDoesNotMatchSyntax);
            } else{
                panic!("Expected details::ParseError, got {:?}", error)
            }
        }
    }
}

test_cases! {
    length_for_color:
        check_error "<color>", "24px";
    insufficient_values:
        check_error "<length> <length>", "24px";
    too_many_values:
        check_error "<percentage>", "100% 100%";

}

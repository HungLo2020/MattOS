// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::details::property::function::property_function;
use crate::value::{Color, ColorOperation, Value};

fn check_value(input: &str, expected: Vec<Value>) {
    let mut parser_input = cssparser::ParserInput::new(input);
    let mut parser = cssparser::Parser::new(&mut parser_input);

    let function_name = parser.expect_function().unwrap().as_ref();
    let function = property_function(function_name).unwrap();

    let result = parser.parse_nested_block(|parser| {
        let output = function(parser);
        if let Ok(output_ok) = output {
            Ok(output_ok)
        } else {
            return output;
        }
    });

    match result {
        Ok(values) => assert_eq!(values, expected),
        Err(error) => panic!("{}", error),
    }
}

test_cases! {
    mix:
        check_value "mix(black, white, 0.5)", vec![
            Value::from(Color::modified(&Color::rgba(0, 0, 0, 255), ColorOperation::mix(&Color::rgba(255, 255, 255, 255), 0.5)))
        ];

    mix_alpha:
        check_value "mix(rgba(255, 0, 255, 0.25), rgba(255, 255, 0, 0.75), 0.25)", vec![
            Value::from(Color::modified(&Color::rgba(255, 0, 255, 63), ColorOperation::mix(&Color::rgba(255, 255, 0, 191), 0.25)))
        ];

    custom_color:
        check_value "custom-color('test', 'some', 'arguments')", vec![
            Value::from(Color::custom(String::from("test"), vec![String::from("some"), String::from("arguments")]))
        ];

    modify_color_add:
        check_value "modify-color(black add white)", vec![
            Value::from(Color::modified(&Color::rgba(0, 0, 0, 255), ColorOperation::add(&Color::rgba(255, 255, 255, 255))))
        ];

    modify_color_subtract:
        check_value "modify-color(black subtract white)", vec![
            Value::from(Color::modified(&Color::rgba(0, 0, 0, 255), ColorOperation::subtract(&Color::rgba(255, 255, 255, 255))))
        ];

    modify_color_multiply:
        check_value "modify-color(black multiply white)", vec![
            Value::from(Color::modified(&Color::rgba(0, 0, 0, 255), ColorOperation::multiply(&Color::rgba(255, 255, 255, 255))))
        ];

    modify_color_set_alpha:
        check_value "modify-color(black set-alpha 0.5)", vec![
            Value::from(Color::modified(&Color::rgba(0, 0, 0, 255), ColorOperation::set(None, None, None, Some(127))))
        ];
}

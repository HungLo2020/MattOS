// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use super::syntax::*;
use super::function::*;

use crate::details::unwrap_parse_error;
use crate::details::SourceLocation;
use crate::details::{parse_error, ParseError, ParseErrorKind};
use crate::value::{Color, Dimension, Value, Unit};

#[derive(Debug, PartialEq)]
pub(super) enum ParseValuesResult {
    Single(Vec<Value>),
    SpaceSeparated(Vec<Value>),
    CommaSeparated(Vec<Value>),
}

impl Into<Vec<Value>> for ParseValuesResult {
    fn into(self) -> Vec<Value> {
        match self {
            Self::Single(values) => values,
            Self::SpaceSeparated(values) => values,
            Self::CommaSeparated(values) => values,
        }
    }
}

type ParseValueComponentResult<'i> = Result<Value, cssparser::ParseError<'i, ParseError>>;

fn parse_dimension<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i> {
    let token = parser.next()?.clone();
    match token {
        cssparser::Token::Dimension{has_sign: _, value, int_value: _, unit: unit_string} => {
            let unit = Unit::parse(unit_string.to_string().as_str());
            match unit {
                Unit::Unknown | Unit::Unsupported => {
                    return parse_error(parser, ParseErrorKind::InvalidPropertyValue, format!("Invalid unit for dimension: {}", unit_string));
                }
                _ => {
                    return Ok(Value::from(Dimension{value, unit}));
                }
            }
        },
        cssparser::Token::Percentage { has_sign: _, unit_value, int_value: _ } => {
            return Ok(Value::from(Dimension{value: unit_value, unit: Unit::Percent}))
        },
        _ => parse_error(parser, ParseErrorKind::InvalidPropertyValue, String::from("Expected a dimension"))
    }
}

fn parse_color<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i> {
    let color_result = cssparser_color::Color::parse(parser);

    if let Ok(color) = color_result {
        match color {
            cssparser_color::Color::Rgba(rgba) => return Ok(Value::from(Color::from((rgba.red, rgba.green, rgba.blue, rgba.alpha)))),
            cssparser_color::Color::Hsl(hsl) => {
                let rgb = cssparser_color::hsl_to_rgb(hsl.hue.unwrap_or(0.0), hsl.saturation.unwrap_or(0.0), hsl.lightness.unwrap_or(0.0));
                return Ok(Value::from(Color::from((rgb.0, rgb.1, rgb.2, hsl.alpha.unwrap_or(1.0)))))
            }
            cssparser_color::Color::Hwb(hwb) => {
                let rgb = cssparser_color::hwb_to_rgb(hwb.hue.unwrap_or(0.0), hwb.whiteness.unwrap_or(0.0), hwb.blackness.unwrap_or(0.0));
                return Ok(Value::from(Color::from((rgb.0, rgb.1, rgb.2, hwb.alpha.unwrap_or(1.0)))))
            }
            _ => (),
        }
    }

    parse_error(parser, ParseErrorKind::InvalidPropertyValue, format!("Input could not be parsed as color"))
}

fn parse_number<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i> {
    let number = parser.expect_number()?;
    Ok(Value::from(number))
}

fn parse_integer<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i> {
    let integer = parser.expect_integer()?;
    Ok(Value::from(integer))
}

fn parse_string<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i> {
    let token = parser.next()?.clone();
    match token {
        cssparser::Token::Ident(value) => {
            return Ok(Value::from(value.as_ref()))
        },
        cssparser::Token::QuotedString(value) => {
            return Ok(Value::from(value.as_ref()))
        }
        _ => {
            parse_error(parser, ParseErrorKind::InvalidPropertyValue, format!("Unexpected token {:?}", token))
        }
    }
}

fn parse_url<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i> {
    let url = parser.expect_url()?;
    return Ok(Value::new_url(url.as_ref()));
}

fn parse_function<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> Result<Vec<Value>, cssparser::ParseError<'i, ParseError>> {
    let function_name = parser.expect_function()?.to_string();

    if let Some(func) = property_function(function_name.as_ref()) {
        parser.parse_nested_block(|parser| {
            let output = func(parser);
            if let Ok(output_ok) = output {
                Ok(output_ok)
            } else {
                return output;
            }
        })
    } else {
        parse_error(parser, ParseErrorKind::UnknownFunction, format!("Unknown function {:?}", function_name))
    }
}

fn parse_value_component<'i, 't>(parser: &mut cssparser::Parser<'i, 't>) -> Result<Vec<Value>, cssparser::ParseError<'i, ParseError>> {
    const PARSE_FUNCTIONS: [for<'i, 't> fn(&mut cssparser::Parser<'i, 't>) -> ParseValueComponentResult<'i>; 6] = [
        parse_integer,
        parse_number,
        parse_color,
        parse_dimension,
        parse_string,
        parse_url,
    ];

    for function in PARSE_FUNCTIONS {
        if let Ok(value) = parser.try_parse(function) {
            return Ok(vec![value])
        }
    }

    let function_result = parse_function(parser);
    if let Ok(values) = function_result {
        return Ok(values)
    } else if let Some(_) = unwrap_parse_error(&function_result) {
        return function_result;
    }

    parse_error(parser, ParseErrorKind::InvalidPropertyValue, String::from("Could not parse input"))
}

pub fn parse_values<'i, 't>(syntax: &ParsedPropertySyntax, parser: &mut cssparser::Parser<'i, 't>) -> Result<Vec<Value>, cssparser::ParseError<'i, ParseError>> {
    let result = parser.parse_until_before(cssparser::Delimiter::Bang, |parser| {
        let mut values: Vec<Value> = Vec::new();
        let mut comma_separated = false;

        while !parser.is_exhausted() {
            let result = parse_value_component(parser);
            if let Ok(parsed_values) = result {
                values.extend(parsed_values);
            } else {
                return Err(result.err().unwrap());
            }

            if let Ok(_) = parser.try_parse(|parser| { parser.expect_comma() }) {
                comma_separated = true;
            }
        }

        if values.len() == 1 {
            Ok(ParseValuesResult::Single(values))
        } else if comma_separated {
            Ok(ParseValuesResult::CommaSeparated(values))
        } else {
            Ok(ParseValuesResult::SpaceSeparated(values))
        }
    });

    if let Ok(values) = result {
        let validation_result = validate_syntax(syntax, &values, SourceLocation::from_file_location(parser.current_source_url().unwrap_or("").to_string(), parser.current_source_location()));
        if let Ok(_) = validation_result {
            Ok(values.into())
        } else {
            Err(parser.new_custom_error(validation_result.unwrap_err()))
        }
    } else {
        Err(result.err().unwrap())
    }
}

// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use nom::{
    IResult,
    Parser,
    branch::alt,
    bytes::complete::tag,
    combinator::recognize,
    character::complete::{char, satisfy, space0, digit1},
    error::ErrorKind,
    multi::{many0_count, many1},
    sequence::{delimited, pair, preceded, terminated, separated_pair},
};

use crate::details::{ParseError, ParseErrorKind, SourceLocation};
use super::value::ParseValuesResult;

use crate::value::{Value, ValueData};

struct SyntaxParseError<I>(I, String);

impl<I> std::fmt::Debug for SyntaxParseError<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyntaxParseError")
         .field("message", &self.1)
         .finish()
    }
}

impl<I> nom::error::ParseError<I> for SyntaxParseError<I> {
    fn from_error_kind(input: I, kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::Tag => SyntaxParseError(input, String::from("Input did not match a tag")),
            _ => SyntaxParseError(input, String::from(kind.description()))
        }
    }

    fn append(input: I, kind: ErrorKind, _other: Self) -> Self {
        match kind {
            ErrorKind::Alt => SyntaxParseError(input, String::from("Input matched none of the alternatives")),
            _ => SyntaxParseError(input, String::from("Multiple errors"))
        }
    }
}

type SyntaxParseResult<I, O> = IResult<I, O, SyntaxParseError<I>>;

fn make_error<I, O>(input: I, message: String) -> SyntaxParseResult<I, O> {
    Err(nom::Err::Error(SyntaxParseError(input, message)))
}

fn make_failure<I, O>(input: I, message: String) -> SyntaxParseResult<I, O> {
    Err(nom::Err::Failure(SyntaxParseError(input, message)))
}

#[derive(Debug, PartialEq, Clone)]
pub enum DataType {
    Length,
    Number,
    Percentage,
    LengthPercentage,
    String,
    Color,
    Url,
    Integer,
    Angle,
    Time,
    Resolution,
    TransformFunction,
    CustomIdent,
}

impl std::fmt::Display for DataType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Length => write!(f, "length"),
            Self::Number => write!(f, "number"),
            Self::Percentage => write!(f, "percentage"),
            Self::LengthPercentage => write!(f, "length-percentage"),
            Self::String => write!(f, "string"),
            Self::Color => write!(f, "color"),
            Self::Url => write!(f, "url"),
            Self::Integer => write!(f, "integer"),
            Self::Angle => write!(f, "angle"),
            Self::Time => write!(f, "time"),
            Self::Resolution => write!(f, "resolution"),
            Self::TransformFunction => write!(f, "transform-function"),
            Self::CustomIdent => write!(f, "custom-ident"),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum SyntaxComponent {
    DataType(DataType),
    Keyword(String),
    SpaceSeparatedList(DataType),
    CommaSeparatedList(DataType),
    Repeat{data_type: DataType, minimum: usize, maximum: usize},
    Comma,
}

impl std::fmt::Display for SyntaxComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DataType(data_type) => write!(f, "<{}>", data_type),
            Self::Keyword(keyword) => write!(f, "{}", keyword),
            Self::SpaceSeparatedList(data_type) => write!(f, "<{}>+", data_type),
            Self::CommaSeparatedList(data_type) => write!(f, "<{}>#", data_type),
            Self::Repeat { data_type, minimum, maximum } => write!(f, "<{}>{{{}, {}}}", data_type, minimum, maximum),
            Self::Comma => write!(f, ",")
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum SyntaxGroup {
    Component(SyntaxComponent),
    Expression(Vec<SyntaxAlternatives>),
}

impl std::fmt::Display for SyntaxGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Component(component) => write!(f, "{}", component),
            Self::Expression(expression) => write!(f, "({})", expression_to_string(expression, " ")),
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum SyntaxAlternatives {
    Component(SyntaxComponent),
    Group(SyntaxGroup),
    Alternatives(Vec<SyntaxGroup>),
}

impl std::fmt::Display for SyntaxAlternatives {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Component(component) => write!(f, "{}", component),
            Self::Group(group) => write!(f, "{}", group),
            Self::Alternatives(alternatives) => write!(f, "{}", expression_to_string(alternatives, " | ")),
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone)]
pub enum ParsedPropertySyntax {
    #[default] Empty,
    Universal,
    Expression(Vec<SyntaxAlternatives>),
}


fn expression_to_string<T>(expression: &[T], separator: &str) -> String
    where T: std::fmt::Display {
    expression.iter().map(|a| { format!("{}", a)}).collect::<Vec<String>>().join(separator).to_string()
}

/*
 * This implements an extended version of the custom property syntax, according
 * to the following EBNF:
 *
 * custom_ident_start ::= [A-Z] | [a-z] | "_" | (a range of unicode symbols)
 * custom_ident ::= custom_ident_start | [0-9] | "-"
 * keyword ::= custom_ident_start custom_ident*
 * data_type_name ::= "length-percentage" | "length" | (several other support data types)
 * data_type ::= "<" data_type_name ">"
 * space_separated_list ::= data_type "+"
 * comma_separated_list ::= data_type "#"
 * repeats ::= data_type "{" [0-9]+ "," [0-9]+ "}"
 * component ::= data_type | keyword | space_separated | comma_separated | repeats
 * group ::= component | ("(" expression ")")
 * alternatives ::= group (" | " group)*
 * expression ::= alternatives (" " alternatives)*
 */

fn custom_ident_start(input: char) -> bool {
    match input {
        'a'..='z' => true,
        'A'..='Z' => true,
        '_' => true,
        '\u{00B7}' => true,
        '\u{00CD}'..='\u{00D7}' => true,
        '\u{00D8}'..='\u{00F6}' => true,
        '\u{00F8}'..='\u{037D}' => true,
        '\u{037F}'..='\u{1FFF}' => true,
        '\u{200C}' => true,
        '\u{200D}' => true,
        '\u{203F}' => true,
        '\u{2040}' => true,
        '\u{2070}'..='\u{218F}' => true,
        '\u{2C00}'..='\u{2FEF}' => true,
        '\u{3001}'..='\u{D7FF}' => true,
        '\u{F900}'..='\u{FDCF}' => true,
        '\u{FDF0}'..='\u{FFFD}' => true,
        '\u{10000}'..='\u{10FFFF}' => true,
        _ => false,
    }
}

fn custom_ident(input: char) -> bool {
    if custom_ident_start(input) || input.is_numeric() || input == '-' {
        true
    } else {
        false
    }
}

fn keyword(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    let result = recognize(
        pair(
            satisfy(custom_ident_start),
            many0_count::<&str, SyntaxParseError<&str>, _>(satisfy(custom_ident))
        )
    ).parse(input);

    if let Ok((remain, keyword)) = result {
        Ok((remain, SyntaxComponent::Keyword(keyword.to_string())))
    } else {
        make_error(input, String::from("Input is not a keyword"))
    }
}

fn data_type_name(input: &str) -> SyntaxParseResult<&str, &str> {
    let result = alt((
        tag::<&str, &str, SyntaxParseError<_>>("length-percentage"),
        tag("length"),
        tag("number"),
        tag("percentage"),
        tag("string"),
        tag("color"),
        tag("url"),
        tag("integer"),
        tag("angle"),
        tag("time"),
        tag("resolution"),
        tag("transform-function"),
        tag("custom-ident"),
    )).parse(input);

    if let Ok((remain, name)) = result {
        Ok((remain, name))
    } else {
        make_error(input, String::from("Input is not a valid data type name"))
    }
}

fn data_type(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    let result = delimited(char('<'), data_type_name, char('>')).parse(input);

    if let Ok((remain, name)) = result {
        let data_type = match name {
            "length-percentage" => DataType::LengthPercentage,
            "length" => DataType::Length,
            "number" => DataType::Number,
            "percentage" => DataType::Percentage,
            "string" => DataType::String,
            "color" => DataType::Color,
            "url" => DataType::Url,
            "integer" => DataType::Integer,
            "angle" => DataType::Angle,
            "time" => DataType::Time,
            "resolution" => DataType::Resolution,
            "transform-function" => DataType::TransformFunction,
            "custom-ident" => DataType::CustomIdent,
            _ => return make_failure(input, String::from("Invalid data type"))
        };
        Ok((remain, SyntaxComponent::DataType(data_type)))
    } else {
        make_error(input, String::from("Input is not a data type"))
    }
}

fn space_separated_list(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    let (remain, data_type) = terminated(data_type, char('+')).parse(input)?;
    if let SyntaxComponent::DataType(data_type_name) = data_type {
        Ok((remain, SyntaxComponent::SpaceSeparatedList(data_type_name)))
    } else {
        make_error(input, String::from("Input is not a space separated list"))
    }
}

fn comma_separated_list(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    let (remain, data_type) = terminated(data_type, char('#')).parse(input)?;
    if let SyntaxComponent::DataType(data_type_name) = data_type {
        Ok((remain, SyntaxComponent::CommaSeparatedList(data_type_name)))
    } else {
        make_error(input, String::from("Input is not a comma separated list"))
    }
}

fn repeat(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    let result = pair(
        data_type,
        delimited(
            char('{'),
            separated_pair(digit1, char(','), digit1),
            char('}'),
        )
    ).parse(input);

    if let Ok((remain, (data_type, (minimum, maximum)))) = result {
        if let SyntaxComponent::DataType(type_name) = data_type {
            let min: usize = minimum.parse().unwrap();
            let max: usize = maximum.parse().unwrap();
            return Ok((remain, SyntaxComponent::Repeat{data_type: type_name, minimum: min, maximum: max}));
        }
    }

    make_error(input, String::from("Input is not a valid repeat pattern"))
}

fn comma(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    char(',').parse(input).map(|r| (r.0, SyntaxComponent::Comma))
}

fn component(input: &str) -> SyntaxParseResult<&str, SyntaxComponent> {
    let result = delimited(
        space0,
        alt((
            repeat,
            space_separated_list,
            comma_separated_list,
            data_type,
            keyword,
            comma,
        )),
        space0
    ).parse(input);

    if let Ok((remain, component)) = result {
        Ok((remain, component))
    } else {
        make_error(input, String::from("Input did not match any syntax component"))
    }
}

fn group(input: &str) -> SyntaxParseResult<&str, SyntaxGroup> {
    let expression = delimited(
        delimited(space0, char('('), space0),
        expression,
        delimited(space0, char(')'), space0),
    ).parse(input);
    if let Ok((remain, result)) = expression {
        if let ParsedPropertySyntax::Expression(exp) = result {
            return Ok((remain, SyntaxGroup::Expression(exp)));
        }
    }

    let component = component.parse(input);
    if let Ok((remain, comp)) = component {
        Ok((remain, SyntaxGroup::Component(comp)))
    } else {
        make_error(input, String::from("Input did not match a group"))
    }
}

fn alternatives(input: &str) -> SyntaxParseResult<&str, SyntaxAlternatives> {
    let alternatives = pair(group, many1(preceded(char('|'), group))).parse(input);
    if let Ok((remain, result)) = alternatives {
        let mut output = Vec::new();
        output.push(result.0);
        output.extend(result.1);
        return Ok((remain, SyntaxAlternatives::Alternatives(output)));
    }

    let group = group.parse(input);
    if let Ok((remain, group_data)) = group {
        if let SyntaxGroup::Component(comp) = group_data {
            Ok((remain, SyntaxAlternatives::Component(comp)))
        } else {
            Ok((remain, SyntaxAlternatives::Group(group_data)))
        }
    } else {
        make_error(input, String::from("Input did not match an alternatives block"))
    }
}

fn expression(input: &str) ->SyntaxParseResult<&str, ParsedPropertySyntax> {
    let result = many1(alternatives).parse(input);

    if let Ok((remain, alternatives)) = result {
        Ok((remain, ParsedPropertySyntax::Expression(alternatives)))
    } else {
        make_error(input, String::from("Input did not match an expression"))
    }
}

fn universal(input: &str) -> SyntaxParseResult<&str, ParsedPropertySyntax> {
    let result = delimited(space0::<&str, SyntaxParseError<&str>>, char('*'), space0).parse(input);

    if let Ok((remain, _)) = result {
        Ok((remain, ParsedPropertySyntax::Universal))
    } else {
        make_error(input, String::from("Input did not match the universal syntax"))
    }
}

pub fn parse_syntax(input: &str, location: SourceLocation) -> Result<ParsedPropertySyntax, ParseError> {
    let result = alt((
        universal,
        expression,
    )).parse(input);

    if let Ok((_, syntax)) = result {
        Ok(syntax)
    } else {
        match result.err().unwrap() {
            nom::Err::Incomplete(_) => Err(ParseError{ kind: ParseErrorKind::InvalidPropertySyntax, message: String::from("Incomplete input"), location}),
            nom::Err::Error(error) | nom::Err::Failure(error) => {
                let message = format!("Input {} encountered error: {}", error.0, error.1);
                Err(ParseError{ kind: ParseErrorKind::InvalidPropertySyntax, message, location})
            }
        }
    }
}

struct SyntaxValidateError(String);

fn validate_datatype<'a>(datatype: &DataType, values: &'a [Value]) -> Result<&'a [Value], SyntaxValidateError> {
    if let Some((value, remain)) = values.split_first() {
        match datatype {
            DataType::Length => {
                if let ValueData::Dimension(dimension) = &value.data {
                    if dimension.is_length() {
                        return Ok(remain)
                    }
                }
                Err(SyntaxValidateError(format!("Expected Length, got {}", value)))
            },
            DataType::Number => {
                if let ValueData::Dimension(dimension) = &value.data {
                    if dimension.is_number() {
                        return Ok(remain)
                    }
                }
                Err(SyntaxValidateError(format!("Expected Number, got {}", value)))
            },
            DataType::Percentage => {
                if let ValueData::Dimension(dimension) = &value.data {
                    if dimension.is_percent() {
                        return Ok(remain)
                    }
                }
                Err(SyntaxValidateError(format!("Expected Percentage, got {}", value)))
            },
            DataType::LengthPercentage => {
                if let ValueData::Dimension(dimension) = &value.data {
                    if dimension.is_length() || dimension.is_percent() {
                        return Ok(remain)
                    }
                }
                Err(SyntaxValidateError(format!("Expected Length or Percentage, got {}", value)))
            },
            DataType::String => {
                if let ValueData::String(_) = value.data {
                    Ok(remain)
                } else {
                    Err(SyntaxValidateError(format!("Expected String, got {}", value)))
                }
            },
            DataType::Color => {
                if let ValueData::Color(_) = value.data {
                    Ok(remain)
                } else {
                    Err(SyntaxValidateError(format!("Expected Color, got {}", value)))
                }
            },
            DataType::Angle => {
                if let ValueData::Dimension(dimension) = &value.data {
                    if dimension.is_angle() {
                        return Ok(remain);
                    }
                }
                Err(SyntaxValidateError(format!("Expected Angle, got {}", value)))
            },
            DataType::Integer => {
                if let ValueData::Integer(_) = &value.data {
                    return Ok(remain);
                }
                Err(SyntaxValidateError(format!("Expected Integer, got {}", value)))
            },
            DataType::Url => {
                if let ValueData::Url(_) = &value.data {
                    return Ok(remain);
                }
                Err(SyntaxValidateError(format!("Expected URL, got {}", value)))
            },
            _ => {
                Err(SyntaxValidateError(format!("Unhandled data type {:?}", datatype)))
            }
        }
    } else {
        Err(SyntaxValidateError(String::from("Expected a datatype")))
    }
}

fn validate_keyword<'a>(keyword: &String, values: &'a [Value]) -> Result<&'a [Value], SyntaxValidateError> {
    if let Some((value, remain)) = values.split_first() {
        if let ValueData::String(data) = &value.data {
            if data == keyword {
                Ok(remain)
            } else {
                Err(SyntaxValidateError(format!("Unexpected keyword {}", data)))
            }
        } else {
            Err(SyntaxValidateError(format!("{:?} is not a keyword", value)))
        }
    } else {
        Err(SyntaxValidateError(String::from("Expected a keyword")))
    }
}

fn validate_list<'a>(datatype: &DataType, values: &'a [Value], minimum: usize, maximum: usize) -> Result<&'a [Value], SyntaxValidateError> {
    let mut count = 0;
    let mut remain = values;
    while !remain.is_empty() {
        let result = validate_datatype(datatype, remain);
        if let Ok(validate_remain) = result {
            count += 1;
            remain = validate_remain;

            if count == maximum {
                break
            }
        } else {
            return result;
        }
    }

    if count < minimum {
        Err(SyntaxValidateError(format!("Expected at least {} values of type {:?}", minimum, datatype)))
    } else if count > maximum {
        Err(SyntaxValidateError(format!("Expected at most {} values of type {:?}", maximum, datatype)))
    } else {
        Ok(remain)
    }
}

#[derive(Debug, PartialEq)]
enum ListType {
    NotAList,
    SpaceSeparated,
    CommaSeparated,
}

fn validate_component<'a>(component: &SyntaxComponent, values: &'a [Value], list_type: &ListType) -> Result<&'a [Value], SyntaxValidateError> {
    match component {
        SyntaxComponent::DataType(datatype) => validate_datatype(datatype, values),
        SyntaxComponent::Keyword(keyword) => validate_keyword(keyword, values),
        SyntaxComponent::Comma => Ok(values),
        SyntaxComponent::SpaceSeparatedList(datatype) => {
            if list_type == &ListType::CommaSeparated {
                return Err(SyntaxValidateError(format!("Expected space separated list, got comma separated")))
            }

            validate_list(datatype, values, 0, usize::max_value())
        },
        SyntaxComponent::CommaSeparatedList(datatype) => {
            if list_type == &ListType::SpaceSeparated {
                return Err(SyntaxValidateError(format!("Expected comma separated list, got space separated")))
            }

            validate_list(datatype, values, 0, usize::max_value())
        },
        SyntaxComponent::Repeat { data_type, minimum, maximum } => {
            if list_type == &ListType::CommaSeparated {
                return Err(SyntaxValidateError(format!("Expected space separated list, got comma separated")))
            }
            validate_list(data_type, values, *minimum, *maximum)
        },
    }
}

fn validate_group<'a>(group: &SyntaxGroup, values: &'a [Value], list_type: &ListType) -> Result<&'a [Value], SyntaxValidateError> {
    match group {
        SyntaxGroup::Component(component) => validate_component(component, values, list_type),
        SyntaxGroup::Expression(expression) => validate_expression(expression, values, list_type),
    }
}

fn validate_alternatives<'a>(alternatives: &SyntaxAlternatives, values: &'a [Value], list_type: &ListType) -> Result<&'a [Value], SyntaxValidateError> {
    match alternatives {
        SyntaxAlternatives::Component(component) => validate_component(component, values, list_type),
        SyntaxAlternatives::Group(group) => validate_group(group, values, list_type),
        SyntaxAlternatives::Alternatives(alternatives) => {
            for group in alternatives {
                if let Ok(remain) = validate_group(group, values, list_type) {
                    return Ok(remain);
                }
            }
            Err(SyntaxValidateError(format!("None of the alternatives matched")))
        }
    }
}

fn validate_expression<'a>(expression: &[SyntaxAlternatives], values: &'a [Value], list_type: &ListType) -> Result<&'a [Value], SyntaxValidateError> {
    let mut remaining_values = values;
    let mut remaining_expression = expression;

    while !remaining_values.is_empty() && !remaining_expression.is_empty() {
        let alternative: &SyntaxAlternatives;

        if let Some((alt, remain)) = remaining_expression.split_first() {
            alternative = alt;
            remaining_expression = remain;
        } else {
            break;
        }

        let result = validate_alternatives(alternative, remaining_values, list_type);
        if let Ok(remain) = result {
            remaining_values = remain;
        } else {
            return result;
        }
    }

    if remaining_expression.is_empty() {
        Ok(remaining_values)
    } else {
        Err(SyntaxValidateError(format!("Unexpected end of input, expected: {}", expression_to_string(remaining_expression, " "))))
    }
}

pub(super) fn validate_syntax(syntax: &ParsedPropertySyntax, values_result: &ParseValuesResult, location: SourceLocation) -> Result<(), ParseError> {
    let expression = match syntax {
        ParsedPropertySyntax::Empty | ParsedPropertySyntax::Universal => return Ok(()),
        ParsedPropertySyntax::Expression(expression) => expression,
    };

    let values: &[Value];
    let list_type: ListType;
    match values_result {
        ParseValuesResult::Single(v) => {
            values = v;
            list_type = ListType::NotAList;
        },
        ParseValuesResult::SpaceSeparated(v) => {
            values = v;
            list_type = ListType::SpaceSeparated;
        },
        ParseValuesResult::CommaSeparated(v) => {
            values = v;
            list_type = ListType::CommaSeparated;
        }
    }

    let result = validate_expression(expression, values, &list_type);
    if let Ok(remain) = result {
        if remain.is_empty() {
            Ok(())
        } else {
            Err(ParseError{ kind: ParseErrorKind::PropertyValueDoesNotMatchSyntax, message: format!("Received too many values, remaining: {:?}", remain), location})
        }
    } else {
        let error = result.unwrap_err();
        Err(ParseError { kind: ParseErrorKind::PropertyValueDoesNotMatchSyntax, message: error.0, location })
    }
}

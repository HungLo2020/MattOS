// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

// Implements the parts of cssparser that are required to parse things.

use std::sync::Arc;

use cssparser::{CowRcStr, RuleBodyParser};

use crate::property::{add_property_definition, property_definition, Property, PropertyDefinition};
use crate::selector::Selector;

use super::{parse_error, ParseError, ParseErrorKind};
use super::selectorparser::{SelectorParser, ParseRelative};
use super::property::syntax::ParsedPropertySyntax;
use super::property::definitionparser::parse_property_definition;
use super::property::value::parse_values;

#[derive(Debug)]
pub struct ParsedRule {
    pub selectors: Vec<Selector>,
    pub properties: Vec<Property>,
    pub nested_rules: Vec<Self>,
}

#[derive(Debug)]
pub enum ParseResult {
    Property(Property),
    Rule(ParsedRule),
    PropertyDefinition(PropertyDefinition),
    Import(String),
}

#[derive(Debug)]
pub enum AtRulePrelude {
    Property(String),
    Import(String),
}

pub struct RulesParser<const TOP_LEVEL: bool>;
pub type TopLevelParser = RulesParser<true>;
pub type NestedParser = RulesParser<false>;

impl<'i, const TOP_LEVEL: bool> cssparser::QualifiedRuleParser<'i> for RulesParser<TOP_LEVEL> {
    type Prelude = Vec<Selector>;
    type QualifiedRule = ParseResult;
    type Error = ParseError;

    fn parse_prelude<'t>(&mut self, parser: &mut cssparser::Parser<'i, 't>) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        let selector_parser = SelectorParser{};
        let relative = if TOP_LEVEL { ParseRelative::No } else { ParseRelative::Nested };
        let result = selector_parser.parse(parser, relative);
        match result {
            Ok(selectors) => Ok(selectors),
            Err(error) => {
                if let cssparser::ParseErrorKind::Custom(error) = error.kind {
                    parse_error(parser, ParseErrorKind::InvalidSelectors, format!("Invalid Selectors: {}", error.message))
                } else {
                    Err(error)
                }
            }
        }
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _location: &cssparser::ParserState,
        parser: &mut cssparser::Parser<'i, 't>) -> Result<Self::QualifiedRule, cssparser::ParseError<'i, Self::Error>>
    {
        let mut nested_parser = NestedParser{};
        let mut body_parser = RuleBodyParser::<NestedParser, Self::QualifiedRule, Self::Error>::new(parser, &mut nested_parser);

        let mut properties = Vec::new();
        let mut nested = Vec::new();
        while let Some(entry) = body_parser.next() {
            if let Ok(entry_contents) = entry {
                match entry_contents {
                    ParseResult::Property(property) => properties.push(property),
                    ParseResult::Rule(rule) => nested.push(rule),
                    ParseResult::PropertyDefinition(definition) => {
                        add_property_definition(&Arc::new(definition));
                    },
                    ParseResult::Import(_) => return parse_error(parser, ParseErrorKind::UnsupportedAtRule, String::from("@import can only be used at top level")),
                }
            } else {
                return Err(entry.unwrap_err().0)
            }
        }

        Ok(ParseResult::Rule(ParsedRule {
            selectors: prelude,
            properties,
            nested_rules: nested,
        }))
    }
}

impl<'i, const TOP_LEVEL: bool> cssparser::AtRuleParser<'i> for RulesParser<TOP_LEVEL> {
    type Prelude = AtRulePrelude;
    type AtRule = ParseResult;
    type Error = ParseError;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        let name_string = name.to_string();
        match name_string.as_str() {
            "property" => {
                return Ok(AtRulePrelude::Property(input.expect_ident()?.to_string()));
            },
            "import" => {
                let url = input.expect_url_or_string()?.to_string();
                return Ok(AtRulePrelude::Import(url));
            }
            _ => parse_error(input, ParseErrorKind::UnsupportedAtRule, format!("Unsupported @-rule {}", name)),
        }
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &cssparser::ParserState,
        input: &mut cssparser::Parser<'i, 't>,
    ) -> Result<Self::AtRule, cssparser::ParseError<'i, Self::Error>> {
        match prelude {
            AtRulePrelude::Property(name) => {
                let result = parse_property_definition(input, name.to_string());
                match result {
                    Ok(definition) => return Ok(ParseResult::PropertyDefinition(definition)),
                    Err(error) => return parse_error(input, ParseErrorKind::InvalidPropertyDefinition, error.to_string())
                }
            },
            _ => {
                return parse_error(input, ParseErrorKind::UnsupportedAtRule, format!("Got @-rule: {:?}", prelude));
            }
        }
    }

    fn rule_without_block(
        &mut self,
        prelude: Self::Prelude,
        _start: &cssparser::ParserState,
    ) -> Result<Self::AtRule, ()> {
        match prelude {
            AtRulePrelude::Import(url) => {
                return Ok(ParseResult::Import(url))
            },
            _ => {
                return Err(())
            }
        }
    }
}

impl<'i, const TOP_LEVEL: bool> cssparser::DeclarationParser<'i> for RulesParser<TOP_LEVEL> {
    type Declaration = ParseResult;
    type Error = ParseError;

    fn parse_value<'t>(&mut self, name: CowRcStr<'i>, input: &mut cssparser::Parser<'i, 't>, _state: &cssparser::ParserState) -> Result<Self::Declaration, cssparser::ParseError<'i, Self::Error>> {
        let definition = property_definition(name.to_string().as_str());
        if definition.is_none() {
            if !name.starts_with("--") {
                return parse_error(input, ParseErrorKind::UnknownProperty, format!("No definition for property {}", name));
            }

            let values_result = parse_values(&ParsedPropertySyntax::Universal, input);
            match values_result {
                Ok(values) => {
                    return Ok(ParseResult::PropertyDefinition(PropertyDefinition {
                        name: name.to_string(),
                        syntax: ParsedPropertySyntax::Universal,
                        inherit: false,
                        initial: values,
                    }));
                }
                Err(error) => {
                    return parse_error(input, ParseErrorKind::InvalidPropertyValue, format!("Parsing values for property {} failed: {}", name, error));
                }
            }
        }

        let pd = definition.unwrap();
        let values_result = parse_values(&pd.syntax, input);
        match values_result {
            Ok(values) => {
                Ok(ParseResult::Property(Property {
                    name: name.to_string(),
                    definition: pd,
                    values,
                }))
            }
            Err(error) => {
                if let cssparser::ParseErrorKind::Custom(error) = error.kind {
                    parse_error(input, ParseErrorKind::InvalidPropertyValue, format!("Parsing values for property {} failed: {}", name, error.message))
                } else {
                    Err(error)
                }
            }
        }
    }
}

impl<'i, const TOP_LEVEL: bool> cssparser::RuleBodyItemParser<'i, ParseResult, ParseError>
    for RulesParser<TOP_LEVEL>
{
    fn parse_declarations(&self) -> bool {
        true
    }

    fn parse_qualified(&self) -> bool {
        true
    }
}

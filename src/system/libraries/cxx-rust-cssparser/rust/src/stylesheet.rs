// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use std::sync::Arc;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::details::parse_error_from_cssparser_error;
use crate::details::rulesparser::*;
use crate::parseerror::{ParseError, ParseErrorKind, SourceLocation};

use crate::property::add_property_definition;
use crate::stylerule::*;

#[derive(Debug)]
pub struct StyleSheet {
    pub path: PathBuf,
    pub rules: Vec<StyleRule>,
    pub errors: Vec<ParseError>,
    pub imported_sheets: Vec<StyleSheet>,
}

impl StyleSheet {
    pub fn new(path: PathBuf) -> StyleSheet {
        StyleSheet {
            path,
            rules: Vec::new(),
            errors: Vec::new(),
            imported_sheets: Vec::new(),
        }
    }

    pub fn all_rules(&self) -> Vec<StyleRule> {
        let mut rules: Vec<_> = self.imported_sheets.iter().map(|sheet| sheet.all_rules()).flatten().collect();
        rules.extend(self.rules.clone());
        rules
    }

    pub fn all_errors(&self) -> Vec<ParseError> {
        let mut errors: Vec<_> = self.imported_sheets.iter().map(|sheet| sheet.all_errors()).flatten().collect();
        errors.extend(self.errors.clone());
        errors
    }

    pub fn all_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<_> = self.imported_sheets.iter().map(|sheet| sheet.all_paths()).flatten().collect();
        paths.push(self.path.clone());
        paths
    }

    pub fn parse(&mut self) -> Result<(), ParseError> {
        let file = File::open(&self.path);
        if let Err(error) = file {
            return Err(ParseError{ kind: ParseErrorKind::FileError, message: format!("{}", error), location: SourceLocation{ file: self.path.to_string_lossy().to_string(), line: 0, column: 0 } });
        }

        let mut data = String::new();
        let result = file.unwrap().read_to_string(&mut data);
        if let Err(error) = result {
            return Err(ParseError{ kind: ParseErrorKind::FileError, message: format!("{}", error), location: SourceLocation{ file: self.path.to_string_lossy().to_string(), line: 0, column: 0 } });
        }

        self.parse_string(data.as_str())
    }

    pub fn parse_string(&mut self, input: &str) -> Result<(), ParseError> {
        let prefix_input = format!("/*# sourceURL={} */\n{}", self.path.to_string_lossy().to_string(), input);
        let mut parser_input = cssparser::ParserInput::new(prefix_input.as_str());
        let mut parser = cssparser::Parser::new(&mut parser_input);
        let mut rules_parser = TopLevelParser{};
        let style_sheet_parser = cssparser::StyleSheetParser::new(&mut parser, &mut rules_parser);

        let mut rules: Vec<StyleRule> = Vec::new();
        let mut errors: Vec<ParseError> = Vec::new();
        for entry in style_sheet_parser {
            match entry {
                Ok(entry_contents) => {
                    match entry_contents {
                        ParseResult::Rule(rule) => {
                            let mut parsed_rules = StyleRule::from_parsed_rule(&rule, self);
                            rules.append(&mut parsed_rules);
                        },
                        ParseResult::PropertyDefinition(definition) => {
                            let arc = Arc::new(definition);
                            add_property_definition(&arc);
                        },
                        ParseResult::Import(name) => {
                            self.import(PathBuf::from(name))?
                        }
                        ParseResult::Property(_) => {
                            panic!("Received property at toplevel!");
                        }
                    }
                }
                Err(error) => {
                    errors.push(parse_error_from_cssparser_error(&error.0, self.path.to_string_lossy().to_string()));
                }
            }
        }

        self.rules.extend(rules);
        self.errors.extend(errors);

        Ok(())
    }

    pub fn import(&mut self, file: PathBuf) -> Result<(), ParseError> {
        let path = if file.is_absolute() { file.clone() } else { self.path.parent().unwrap().join(file.clone()) };
        let mut sheet = StyleSheet::new(path);
        sheet.parse()?;

        self.imported_sheets.push(sheet);

        Ok(())
    }
}

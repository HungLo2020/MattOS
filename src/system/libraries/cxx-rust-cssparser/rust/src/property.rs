// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use std::sync::{Arc, RwLock, OnceLock};

use crate::{
    details::property::syntax::{parse_syntax, ParsedPropertySyntax},
    parseerror::{ParseError, SourceLocation},
    value::Value
};

#[derive(Debug, Default, PartialEq, Clone)]
pub struct PropertyDefinition {
    pub name: String,
    pub syntax: ParsedPropertySyntax,
    pub inherit: bool,
    pub initial: Vec<Value>,
}

fn property_definitions() -> &'static RwLock<Vec<Arc<PropertyDefinition>>> {
    static DEFINITIONS: OnceLock<RwLock<Vec<Arc<PropertyDefinition>>>> = OnceLock::new();
    DEFINITIONS.get_or_init(|| RwLock::new(Vec::new()))
}

pub fn property_definition(name: &str) -> Option<Arc<PropertyDefinition>> {
    if let Ok(definitions) = property_definitions().read() {
        let def = definitions.iter().find(|&definition| definition.name == name);
        if let Some(definition) = def {
            return Some(definition.clone());
        }
    }

    None{}
}

pub fn add_property_definition(definition: &Arc<PropertyDefinition>) -> bool {
    let defs = property_definitions().write();
    if let Ok(mut definitions) = defs {
        if definitions.iter().find(|&def| def.name == definition.name).is_some() {
            return false;
        }

        definitions.push(definition.clone());
    }

    true
}

impl PropertyDefinition {
    pub fn empty() -> PropertyDefinition {
        PropertyDefinition {
            name: String::new(),
            syntax: ParsedPropertySyntax::Empty,
            inherit: false,
            initial: Vec::new(),
        }
    }

    pub fn from_name_syntax(name: &str, syntax: &str, file: &str, line: u32, column: u32) -> Result<PropertyDefinition, ParseError> {
        let result = parse_syntax(syntax, SourceLocation { file: file.to_string(), line, column });
        if let Ok(parsed_syntax) = result {
            Ok(
                PropertyDefinition {
                    name: String::from(name),
                    syntax: parsed_syntax,
                    inherit: false,
                    initial: Vec::new(),
                }
            )
        } else {
            Err(result.err().unwrap())
        }
    }

    pub fn from_name_syntax_initial(name: &str, syntax: &str, initial: &[Value], file: &str, line: u32, column: u32) -> Result<PropertyDefinition, ParseError> {
        let mut pd = Self::from_name_syntax(name, syntax, file, line, column)?;
        pd.initial = Vec::from(initial);
        Ok(pd)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Property {
    pub name: String,
    pub definition: Arc<PropertyDefinition>,
    pub values: Vec<Value>,
}

// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2024 Arjen Hiemstra <ahiemstra@heimr.nl>

use cssparser::color::{parse_named_color, parse_hash_color, clamp_unit_f32};

use crate::ffi::{SelectorKind, ValueType, Color};

#[derive(Debug)]
pub struct Error(&'static str);

impl std::error::Error for Error {
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}



#[derive(Debug, Clone)]
pub struct SelectorPart {
    pub kind: SelectorKind,
    pub value: Value,
}

impl SelectorPart {
    pub fn new(kind: SelectorKind, value: Value) -> SelectorPart {
        SelectorPart{kind, value}
    }

    pub fn kind(&self) -> SelectorKind {
        self.kind
    }

    pub fn value(&self) -> &Value {
        &self.value
    }
}

#[derive(Debug, Clone)]
pub struct Selector {
    pub parts: Vec<SelectorPart>,
}

impl Selector {
    pub fn new() -> Selector {
        Selector{parts: Vec::new()}
    }

    pub fn parts(&self) -> Vec<SelectorPart> {
        self.parts.clone()
    }
}

#[derive(Debug, Default, Clone)]
pub struct Property {
    pub name: String,
    pub values: Vec<Value>,
}

impl Property {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn values(&self) -> Vec<Value> {
        self.values.clone()
    }
}

#[derive(Debug)]
pub struct CssRule {
    pub selectors: Vec<Selector>,
    pub properties: Vec<Property>,
}

impl CssRule {
    pub fn selectors(&self) -> Vec<Selector> {
        self.selectors.clone()
    }

    pub fn properties(&self) -> Vec<Property> {
        self.properties.clone()
    }
}

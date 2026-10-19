// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::value::Value;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttributeOperator {
    None,
    Exists,
    Equals,
    Includes,
    Prefixed,
    Suffixed,
    Substring,
    DashMatch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectorKind {
    Unknown,
    AnyElement,
    Type,
    Class,
    Id,
    PseudoClass,
    Attribute,
    RelativeParent,
    DocumentRoot,
    Part,
    DescendantCombinator,
    ChildCombinator,
    PartCombinator,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectorValue {
    Empty,
    Value(Value),
    Attribute{name: String, operator: AttributeOperator, value: Value},
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectorPart {
    pub kind: SelectorKind,
    pub value: SelectorValue,
}

impl SelectorPart {
    pub fn new_with_empty(kind: SelectorKind) -> SelectorPart {
        SelectorPart { kind, value: SelectorValue::Empty }
    }

    pub fn new_with_value(kind: SelectorKind, value: Value) -> SelectorPart {
        SelectorPart { kind, value: SelectorValue::Value(value) }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector {
    pub parts: Vec<SelectorPart>,
}

impl Selector {
    pub fn new() -> Selector {
        Selector { parts: Vec::new() }
    }

    pub fn from_parts(parts: &[SelectorPart]) -> Selector {
        Selector {
            parts: Vec::from(parts),
        }
    }

    pub fn combine(first: &Selector, second: &Selector) -> Selector {
        let mut parts = first.parts.clone();

        if !second.parts.is_empty() {
            // Find all the indices of RelativeParent so we can replace them
            // later on.
            let mut relative_indices = Vec::new();
            for (index, part) in parts.iter().enumerate() {
                if part.kind == SelectorKind::RelativeParent {
                    relative_indices.push(index);
                }
            }

            if relative_indices.is_empty() {
                parts.extend(second.parts.clone())
            } else {
                // Since we modify the list and introduce potentially multiple
                // new items, our indices may shift. We need to keep track of
                // that otherwise the insertion point becomes incorrect.
                let mut offset = 0;
                for index in relative_indices {
                    let i = index + offset;
                    parts.remove(i);
                    parts.splice(i..i, second.parts.clone());
                    offset = offset + second.parts.len() - 1;
                }
            }
        }

        Selector { parts }
    }

    pub fn push_with_empty(&mut self, kind: SelectorKind) {
        self.parts.push(SelectorPart::new_with_empty(kind))
    }

    pub fn push_with_value(&mut self, kind: SelectorKind, value: Value) {
        self.parts.push(SelectorPart::new_with_value(kind, value));
    }
}

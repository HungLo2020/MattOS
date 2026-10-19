// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::property::Property;
use crate::selector::Selector;
use crate::value::ValueData;

use crate::details::rulesparser::ParsedRule;
use crate::stylesheet::StyleSheet;

#[derive(Clone, Debug, PartialEq)]
pub struct StyleRule {
    pub selector: Selector,
    pub properties: Vec<Property>,
}

fn resolve_urls(properties: &Vec<Property>, style_sheet: &StyleSheet) -> Vec<Property> {
    let mut result = properties.clone();

    for property in &mut result {
        for value in &mut property.values {
            if let ValueData::Url(url) = &mut value.data {
                *url = style_sheet.path.parent().unwrap().join(url.clone()).to_string_lossy().to_string()
            }
        }
    }

    result
}

impl StyleRule {
    pub fn from_parsed_rule(parsed: &ParsedRule, style_sheet: &StyleSheet) -> Vec<StyleRule> {
        let mut result = Vec::new();

        for selector in &parsed.selectors {
            if selector.parts.is_empty() && parsed.properties.is_empty() {
                continue;
            }

            result.push(StyleRule {
                selector: selector.clone(),
                properties: resolve_urls(&parsed.properties, style_sheet),
            });

            for nested_rule in &parsed.nested_rules {
                for nested_result in StyleRule::from_parsed_rule(nested_rule, style_sheet) {
                    result.push(Self {
                        selector: Selector::combine(&nested_result.selector, &selector),
                        properties: nested_result.properties,
                    });
                }
            }
        }

        result
    }
}

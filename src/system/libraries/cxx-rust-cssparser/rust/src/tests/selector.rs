// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::selector::*;
use crate::value::Value;

#[test]
fn combine_basic() {
    let first = Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type")),
    ]);

    let second = Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
    ]);

    let combined = Selector::combine(&first, &second);
    assert_eq!(combined, Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type")),
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
    ]));
}

#[test]
fn combine_nested() {
    let first = Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type")),
    ]);

    let second = Selector::from_parts(&[
        SelectorPart::new_with_empty(SelectorKind::RelativeParent),
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
    ]);

    let combined = Selector::combine(&second, &first);
    assert_eq!(combined, Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type")),
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
    ]));
}

#[test]
fn combine_multiple() {
    let first = Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type2")),
        SelectorPart::new_with_empty(SelectorKind::DescendantCombinator),
        SelectorPart::new_with_empty(SelectorKind::RelativeParent),
        SelectorPart::new_with_empty(SelectorKind::DescendantCombinator),
        SelectorPart::new_with_empty(SelectorKind::RelativeParent),
    ]);

    let second = Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type1")),
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
    ]);

    let combined = Selector::combine(&first, &second);
    assert_eq!(combined, Selector::from_parts(&[
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type2")),
        SelectorPart::new_with_empty(SelectorKind::DescendantCombinator),
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type1")),
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
        SelectorPart::new_with_empty(SelectorKind::DescendantCombinator),
        SelectorPart::new_with_value(SelectorKind::Type, Value::from("type1")),
        SelectorPart::new_with_value(SelectorKind::Class, Value::from("class")),
    ]));
}

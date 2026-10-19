// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::details::property::syntax::*;
use crate::parseerror::SourceLocation;

fn check_syntax(input: &str, expected: ParsedPropertySyntax) {
    let result = parse_syntax(input, SourceLocation::from_file("Test Input"));

    match result {
        Ok(syntax) => assert_eq!(syntax, expected),
        Err(error) => panic!("{}", error),
    }
}

test_cases! {
    single_datatype:
        check_syntax "<color>",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Color))
        ]);

    space_separated_list:
        check_syntax "<length>+",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Component(SyntaxComponent::SpaceSeparatedList(DataType::Length))
        ]);

    comma_separated_list:
        check_syntax "<url>#",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Component(SyntaxComponent::CommaSeparatedList(DataType::Url))
        ]);

    multiple:
        check_syntax "<percentage> <angle>",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Component(
                SyntaxComponent::DataType(DataType::Percentage),
            ),
            SyntaxAlternatives::Component(
                SyntaxComponent::DataType(DataType::Angle),
            ),
        ]);

    keywords_list:
        check_syntax "one | two | three",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Alternatives(vec![
                SyntaxGroup::Component(SyntaxComponent::Keyword(String::from("one"))),
                SyntaxGroup::Component(SyntaxComponent::Keyword(String::from("two"))),
                SyntaxGroup::Component(SyntaxComponent::Keyword(String::from("three"))),
            ])
        ]);

    any:
        check_syntax "*",
        ParsedPropertySyntax::Universal;

    type_or_keyword:
        check_syntax "<time> | auto",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Alternatives(vec![
                SyntaxGroup::Component(SyntaxComponent::DataType(DataType::Time)),
                SyntaxGroup::Component(SyntaxComponent::Keyword(String::from("auto"))),
            ])
        ]);

    repeat:
        check_syntax "<length>{1,4}",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Component(SyntaxComponent::Repeat{data_type: DataType::Length, minimum: 1, maximum: 4})
        ]);

    group:
        check_syntax "(auto | <length>) | (<length> <length>) | (<length> <length> <length> <length>)",
        ParsedPropertySyntax::Expression(vec![
            SyntaxAlternatives::Alternatives(vec![
                SyntaxGroup::Expression(vec![
                    SyntaxAlternatives::Alternatives(vec![
                        SyntaxGroup::Component(SyntaxComponent::Keyword(String::from("auto"))),
                        SyntaxGroup::Component(SyntaxComponent::DataType(DataType::Length))
                    ])
                ]),
                SyntaxGroup::Expression(vec![
                    SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Length)),
                    SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Length)),
                ]),
                SyntaxGroup::Expression(vec![
                    SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Length)),
                    SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Length)),
                    SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Length)),
                    SyntaxAlternatives::Component(SyntaxComponent::DataType(DataType::Length)),
                ]),
            ])
        ]);
}

#[test]
fn invalid_datatype() {
    let result = parse_syntax("<invalid>", SourceLocation::from_file("Test Input"));
    assert!(result.is_err());

    let result = parse_syntax("<invalid> | <length>", SourceLocation::from_file("Test Input"));
    assert!(result.is_err());
}

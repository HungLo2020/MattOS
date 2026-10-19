// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    Unspecified,
    Unimplemented,
    UnexpectedEndOfInput,
    Unknown,
    UnknownProperty,
    UnexpectedToken,
    InvalidSelectors,
    InvalidPropertySyntax,
    InvalidPropertyValue,
    UnknownFunction,
    InvalidPropertyDefinition,
    PropertyValueDoesNotMatchSyntax,
    UnsupportedAtRule,
    InvalidAtRule,
    InvalidQualifiedRule,
    FileError,
    StyleSheetParseError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl SourceLocation {
    pub fn from_file_location(file: String, location: cssparser::SourceLocation) -> SourceLocation {
        SourceLocation { file, line: location.line, column: location.column }
    }

    pub fn from_file(file: &str) -> SourceLocation {
        SourceLocation { file: file.to_string(), line: 0, column: 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String,
    pub location: SourceLocation,
}

impl std::error::Error for ParseError {
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "In file \"{}\" at line {} column {}: ", self.location.file, self.location.line, self.location.column)?;
        match self.kind {
            ParseErrorKind::Unspecified => write!(f, "Unspecified error"),
            ParseErrorKind::Unimplemented => write!(f, "Feature is not yet implemented"),
            ParseErrorKind::UnexpectedEndOfInput => write!(f, "Unexpected end of input"),
            ParseErrorKind::Unknown => write!(f, "Unknown error: {}", self.message),
            ParseErrorKind::UnknownProperty => write!(f, "Unknown Property: {}", self.message),
            ParseErrorKind::UnexpectedToken => write!(f, "Unexpected Token: {}", self.message),
            ParseErrorKind::InvalidSelectors => write!(f, "Invalid Selectors: {}", self.message),
            ParseErrorKind::InvalidPropertySyntax => write!(f, "Invalid property syntax: {}", self.message),
            ParseErrorKind::InvalidPropertyValue => write!(f, "Invalid property value: {}", self.message),
            ParseErrorKind::UnknownFunction => write!(f, "Unknown function: {}", self.message),
            ParseErrorKind::InvalidPropertyDefinition => write!(f, "Invalid property definition: {}", self.message),
            ParseErrorKind::PropertyValueDoesNotMatchSyntax => write!(f, "Property value does not match syntax: {}", self.message),
            ParseErrorKind::UnsupportedAtRule => write!(f, "Unsupported @-rule: {}", self.message),
            ParseErrorKind::InvalidAtRule => write!(f, "Invalid @-rule: {}", self.message),
            ParseErrorKind::InvalidQualifiedRule => write!(f, "Invalid qualified rule"),
            ParseErrorKind::FileError => write!(f, "IO Error: {}", self.message),
            ParseErrorKind::StyleSheetParseError => write!(f, "Stylesheet failed to parse: {}", self.message),
        }
    }
}

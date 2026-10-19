// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use std::path::PathBuf;

use ffi::ValueConversionError;

use crate::selector::{Selector, SelectorPart, SelectorKind, SelectorValue};
use crate::parseerror::ParseError;
use crate::property::Property;
use crate::stylerule::StyleRule;
use crate::stylesheet::StyleSheet;
use crate::value;

use crate::value::Value;
use crate::value::{Color, ColorOperation};


#[cxx::bridge(namespace = "cssparser::rust")]
mod ffi {
    #[derive(Debug, Clone, Copy)]
    pub enum Unit {
        Unknown,
        Unsupported,
        Number,
        Px,
        Em,
        Rem,
        Pt,
        Percent,
        Degrees,
        Radians,
        Seconds,
        Milliseconds,
    }

    pub enum ColorType {
        Empty,
        Rgba,
        Custom,
        Modified,
    }

    pub struct Rgba {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    }

    pub struct CustomColor {
        source: String,
        arguments: Vec<String>,
    }

    pub enum ColorOperationType {
        Set,
        Add,
        Subtract,
        Multiply,
        Mix,
    }

    // This uses -1 to indicate the value should not be set, which is why this
    // uses i16 for values.
    pub struct SetColorOperationValues {
        r: i16,
        g: i16,
        b: i16,
        a: i16,
    }

    pub struct MixColorOperationValues {
        other: Box<Color>,
        amount: f32,
    }

    pub struct ModifiedColor {
        color: Box<Color>,
        operation: Box<ColorOperation>,
    }

    pub enum ValueType {
        Empty,
        Dimension,
        String,
        Color,
        Image,
        Url,
        Integer,
    }

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

    #[derive(Debug)]
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

    #[derive(Debug, Clone, Copy)]
    pub struct Dimension {
        value: f32,
        unit: Unit,
    }

    pub struct ValueConversionError {
        message: String,
    }

    pub struct StyleSheetError {
        file: String,
        line: u32,
        column: u32,
        message: String,
    }

    extern "Rust" {
        fn to_string(self: &Dimension) -> String;

        fn operation_type(self: &ModifiedColor) -> ColorOperationType;
        fn color_value(self: &ModifiedColor) -> Result<Box<Color>>;
        fn set_values(self: &ModifiedColor) -> Result<SetColorOperationValues>;
        fn mix_values(self: &ModifiedColor) -> Result<MixColorOperationValues>;

        type ColorOperation;

        type Color;
        fn color_type(self: &Color) -> ColorType;
        fn to_string(self: &Color) -> String;
        fn to_rgba(self: &Color) -> Result<Rgba>;
        fn to_custom(self: &Color) -> Result<CustomColor>;
        fn to_modified(self: &Color) -> Result<ModifiedColor>;

        type Value;
        fn value_type(self: &Value) -> ValueType;
        fn to_dimension(self: &Value) -> Result<Dimension>;
        fn to_string(self: &Value) -> String;
        fn to_color(self: &Value) -> Result<Box<Color>>;
        fn to_image(self: &Value) -> Result<&str>;
        fn to_url(self: &Value) -> Result<&str>;
        fn to_integer(self: &Value) -> Result<i32>;

        type SelectorPart;
        fn kind(self: &SelectorPart) -> SelectorKind;
        fn value(self: &SelectorPart) -> &Value;
        fn attribute_name(self: &SelectorPart) -> String;
        fn attribute_operator(self: &SelectorPart) -> AttributeOperator;
        fn attribute_value(self: &SelectorPart) -> &Value;

        type Selector;
        fn parts(self: &Selector) -> Vec<SelectorPart>;

        type Property;
        fn name(self: &Property) -> String;
        fn values(self: &Property) -> Vec<Value>;

        type StyleRule;
        fn selector(self: &StyleRule) -> &Selector;
        fn properties(self: &StyleRule) -> Vec<Property>;

        type StyleSheet;
        fn rules(self: &StyleSheet) -> Vec<StyleRule>;
        fn errors(self: &StyleSheet) -> Vec<StyleSheetError>;
        fn paths(self: &StyleSheet) -> Vec<String>;
        fn parse(self: &mut StyleSheet) -> Result<()>;
        fn parse_string(self: &mut StyleSheet, data: &str) -> Result<()>;
        fn import_file(self: &mut StyleSheet, path: &str) -> Result<()>;

        fn create_stylesheet(path: &str) -> Box<StyleSheet>;
    }
}

macro_rules! convert_enum {
    ($source:path, $target:path, { $($source_variant:pat => $target_variant:ident),* $(,)? }) => {
        impl From<$source> for $target {
            fn from(value: $source) -> Self {
                match value {
                    $(
                        $source_variant => <$target>::$target_variant,
                    )*
                }
            }
        }
    };
}

convert_enum!(value::ColorData, ffi::ColorType, {
    value::ColorData::Empty => Empty,
    value::ColorData::Rgba{ r: _, g: _, b: _, a: _ } => Rgba,
    value::ColorData::Custom{ source: _, arguments: _ } => Custom,
    value::ColorData::Modified{ color: _, operation: _ } => Modified,
});

convert_enum!(value::ValueData, ffi::ValueType, {
    value::ValueData::Empty => Empty,
    value::ValueData::Dimension(_) => Dimension,
    value::ValueData::String(_) => String,
    value::ValueData::Color(_) => Color,
    value::ValueData::Image(_) => Image,
    value::ValueData::Url(_) => Url,
    value::ValueData::Integer(_) => Integer,
});

convert_enum!(value::Unit, ffi::Unit, {
    value::Unit::Unknown => Unknown,
    value::Unit::Unsupported => Unsupported,
    value::Unit::Number => Number,
    value::Unit::Px => Px,
    value::Unit::Em => Em,
    value::Unit::Rem => Rem,
    value::Unit::Pt => Pt,
    value::Unit::Percent => Percent,
    value::Unit::Degrees => Degrees,
    value::Unit::Radians => Radians,
    value::Unit::Seconds => Seconds,
    value::Unit::Milliseconds => Milliseconds,
});

convert_enum!(crate::selector::AttributeOperator, ffi::AttributeOperator, {
    crate::selector::AttributeOperator::None => None,
    crate::selector::AttributeOperator::Exists => Exists,
    crate::selector::AttributeOperator::Equals => Equals,
    crate::selector::AttributeOperator::Includes => Includes,
    crate::selector::AttributeOperator::Prefixed => Prefixed,
    crate::selector::AttributeOperator::Suffixed => Suffixed,
    crate::selector::AttributeOperator::Substring => Substring,
    crate::selector::AttributeOperator::DashMatch => DashMatch,
});

convert_enum!(SelectorKind, ffi::SelectorKind, {
    SelectorKind::Unknown => Unknown,
    SelectorKind::AnyElement => AnyElement,
    SelectorKind::Type => Type,
    SelectorKind::Class => Class,
    SelectorKind::Id => Id,
    SelectorKind::PseudoClass => PseudoClass,
    SelectorKind::Attribute => Attribute,
    SelectorKind::RelativeParent => RelativeParent,
    SelectorKind::DocumentRoot => DocumentRoot,
    SelectorKind::Part => Part,
    SelectorKind::DescendantCombinator => DescendantCombinator,
    SelectorKind::ChildCombinator => ChildCombinator,
    SelectorKind::PartCombinator => PartCombinator,
});

convert_enum!(value::ColorOperation, ffi::ColorOperationType, {
    value::ColorOperation::Set { r: _, g: _, b: _, a: _ } => Set,
    value::ColorOperation::Add { other: _ } => Add,
    value::ColorOperation::Subtract { other: _ } => Subtract,
    value::ColorOperation::Multiply { other: _ } => Multiply,
    value::ColorOperation::Mix { other: _, amount: _ } => Mix,
});

impl From<&value::Dimension> for ffi::Dimension {
    fn from(value: &value::Dimension) -> Self {
        ffi::Dimension{
            value: value.value,
            unit: value.unit.clone().into(),
        }
    }
}

impl ffi::Dimension {
    fn to_string(&self) -> String {
        format!("{}{:?}", self.value, self.unit)
    }
}

impl ffi::ModifiedColor {
    fn operation_type(&self) -> ffi::ColorOperationType {
        (*self.operation.clone()).into()
    }

    fn color_value(&self) -> Result<Box<Color>, ffi::ValueConversionError> {
        match self.operation.as_ref() {
            value::ColorOperation::Add { other } => Ok(other.clone()),
            value::ColorOperation::Subtract { other } => Ok(other.clone()),
            value::ColorOperation::Multiply { other } => Ok(other.clone()),
            _ => Err(ValueConversionError { message: String::from("Modified color does not have a color value") })
        }
    }

    fn set_values(&self) -> Result<ffi::SetColorOperationValues, ffi::ValueConversionError> {
        if let value::ColorOperation::Set { r, g, b, a } = self.operation.as_ref() {
            Ok(ffi::SetColorOperationValues {
                r: r.map_or(-1, |v| v as i16),
                g: g.map_or(-1, |v| v as i16),
                b: b.map_or(-1, |v| v as i16),
                a: a.map_or(-1, |v| v as i16),
            })
        } else {
            Err(ValueConversionError { message: String::from("Not a set color operation") })
        }
    }

    fn mix_values(&self) -> Result<ffi::MixColorOperationValues, ffi::ValueConversionError> {
        if let value::ColorOperation::Mix { other, amount } = self.operation.as_ref() {
            Ok(ffi::MixColorOperationValues {
                other: other.clone(),
                amount: *amount,
            })
        } else {
            Err(ValueConversionError { message: String::from("Not an add color operation") })
        }
    }
}

impl ffi::StyleSheetError {
    fn from_parse_error(error: &ParseError) -> ffi::StyleSheetError {
        ffi::StyleSheetError{
            file: error.location.file.clone(),
            line: error.location.line,
            column: error.location.column,
            message: error.message.clone(),
        }
    }
}

impl value::Color {
    fn color_type(&self) -> ffi::ColorType {
        self.data.clone().into()
    }

    fn to_string(&self) -> String {
        match &self.data {
            value::ColorData::Empty => format!("Empty"),
            value::ColorData::Rgba{r, g, b, a} => format!("RGBA({}, {}, {}, {})", r, g, b, a),
            value::ColorData::Custom{source, arguments} => format!("Custom({}, {:?})", source, arguments),
            value::ColorData::Modified { color, operation } => format!("Modified({}, {:?})", color.to_string(), operation),
        }
    }

    fn to_rgba(&self) -> Result<ffi::Rgba, ffi::ValueConversionError> {
        if let value::ColorData::Rgba{r ,g , b, a} = &self.data {
            Ok(ffi::Rgba{r: *r, g: *g, b: *b, a: *a})
        } else {
            Err(ValueConversionError{ message: String::from("Not an RGBA color") })
        }
    }

    fn to_custom(&self) -> Result<ffi::CustomColor, ffi::ValueConversionError> {
        if let value::ColorData::Custom{source, arguments} = &self.data {
            Ok(ffi::CustomColor{source: source.clone(), arguments: arguments.clone()})
        } else {
            Err(ValueConversionError{ message: String::from("Not an RGBA color") })
        }
    }

    fn to_modified(&self) -> Result<ffi::ModifiedColor, ffi::ValueConversionError> {
        if let value::ColorData::Modified { color, operation } = &self.data {
            Ok(ffi::ModifiedColor{color: color.clone(), operation: Box::new(operation.clone())})
        } else {
            Err(ValueConversionError{ message: String::from("Not a Modified color") })
        }
    }
}

impl value::Value {
    fn value_type(&self) -> ffi::ValueType {
        self.data.clone().into()
    }

    fn to_dimension(&self) -> Result<ffi::Dimension, ffi::ValueConversionError> {
        if let value::ValueData::Dimension(dimension) = &self.data {
            Ok(dimension.into())
        } else {
            Err(ValueConversionError{ message: String::from("Not a length value") })
        }
    }

    fn to_color(&self) -> Result<Box<Color>, ffi::ValueConversionError> {
        if let value::ValueData::Color(color) = &self.data {
            Ok(Box::new(color.clone()))
        } else {
            Err(ffi::ValueConversionError{ message: String::from("Not a color value") })
        }
    }

    fn to_integer(&self) -> Result<i32, ffi::ValueConversionError> {
        if let value::ValueData::Integer(integer) = self.data {
            Ok(integer)
        } else {
            Err(ffi::ValueConversionError{ message: String::from("Not an integer value") })
        }
    }

    fn to_image(&self) -> Result<&str, ffi::ValueConversionError> {
        Err(ffi::ValueConversionError{ message: String::from("Unimplemented") })
    }

    fn to_url(&self) -> Result<&str, ffi::ValueConversionError> {
        if let value::ValueData::Url(url) = &self.data {
            Ok(url.as_str())
        } else {
            Err(ffi::ValueConversionError{ message: String::from("Not a URL") })
        }
    }
}

impl SelectorPart {
    fn kind(&self) -> ffi::SelectorKind {
        self.kind.into()
    }

    fn value(&self) -> &value::Value {
        if let SelectorValue::Value(value) = &self.value {
            value
        } else {
            Value::empty_ref()
        }
    }

    fn attribute_name(&self) -> String {
        if let SelectorValue::Attribute { name, operator: _, value: _ } = &self.value {
            name.clone()
        } else {
            String::new()
        }
    }

    fn attribute_operator(&self) -> ffi::AttributeOperator {
        if let SelectorValue::Attribute { name: _, operator, value: _ } = self.value {
            ffi::AttributeOperator::from(operator)
        } else {
            ffi::AttributeOperator::None
        }
    }

    fn attribute_value(&self) -> &Value {
        if let SelectorValue::Attribute { name: _, operator: _, value } = &self.value {
            value
        } else {
            Value::empty_ref()
        }
    }
}

impl Selector {
    fn parts(&self) -> Vec<SelectorPart> {
        self.parts.clone()
    }
}

impl Property {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn values(&self) -> Vec<value::Value> {
        self.values.clone()
    }
}

impl StyleRule {
    fn selector(&self) -> &Selector {
        &self.selector
    }

    fn properties(&self) -> Vec<Property> {
        self.properties.clone()
    }
}

impl StyleSheet {
    fn rules(&self) -> Vec<StyleRule> {
        self.all_rules()
    }

    fn errors(&self) -> Vec<ffi::StyleSheetError> {
        self.all_errors().iter().map(|error| ffi::StyleSheetError::from_parse_error(error)).collect()
    }

    fn paths(&self) -> Vec<String> {
        self.all_paths().iter().map(|path| path.to_string_lossy().to_string()).collect()
    }

    fn import_file(&mut self, path: &str) -> Result<(), ParseError> {
        self.import(PathBuf::from(path))
    }
}

fn create_stylesheet(path: &str) -> Box<StyleSheet> {
    Box::new(StyleSheet::new(PathBuf::from(path)))
}

impl std::fmt::Display for ValueConversionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Value could not be converted: {}", self.message)
    }
}

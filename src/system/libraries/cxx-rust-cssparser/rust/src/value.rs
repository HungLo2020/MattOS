// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

use crate::details::identifier::Identifier;

#[derive(Debug, Clone, PartialEq)]
pub enum ColorOperation {
    Set { r: Option<u8>, g: Option<u8>, b: Option<u8>, a: Option<u8> },
    Add { other: Box<Color> },
    Subtract { other: Box<Color> },
    Multiply { other: Box<Color> },
    Mix { other: Box<Color>, amount: f32 },
}

impl ColorOperation {
    pub fn add(color: &Color) -> ColorOperation {
        ColorOperation::Add { other: Box::new(color.clone()) }
    }

    pub fn subtract(color: &Color) -> ColorOperation {
        ColorOperation::Subtract { other: Box::new(color.clone()) }
    }

    pub fn multiply(color: &Color) -> ColorOperation {
        ColorOperation::Multiply { other: Box::new(color.clone()) }
    }

    pub fn set(r: Option<u8>, g: Option<u8>, b: Option<u8>, a: Option<u8>) -> ColorOperation {
        ColorOperation::Set { r, g, b, a }
    }

    pub fn mix(color: &Color, amount: f32) -> ColorOperation {
        ColorOperation::Mix { other: Box::new(color.clone()), amount }
    }

    pub fn display_string(&self, color: &Color) -> String {
        return match self {
            Self::Set { r, g, b, a } => {
                let mut parts = String::new();
                for (name, value) in [("R", r), ("G", g), ("B", b), ("A", a)] {
                    if let Some(value) = value {
                        if !parts.is_empty() {
                            parts += ", ";
                        }
                        parts += format!("{} {}", name, value).as_str();
                    }
                }
                format!("Set {} {}", color, parts)
            },
            Self::Add { other } => format!("{} + {}", color, other),
            Self::Subtract { other } => format!("{} - {}", color, other),
            Self::Multiply { other } => format!("{} * {}", color, other),
            Self::Mix { other, amount } => format!("Mix {} {} {}", color, other, amount),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) enum ColorData {
    #[default] Empty,
    Rgba { r: u8, g: u8, b: u8, a: u8 },
    Custom { source: String, arguments: Vec<String> },
    Modified { color: Box<Color>, operation: ColorOperation },
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Color {
    pub(crate) data: ColorData,
}

impl Color {
    pub fn empty() -> Color {
        Color { data: ColorData::Empty }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8 ) -> Color {
        Color { data: ColorData::Rgba { r, g, b, a } }
    }

    pub fn custom(source: String, arguments: Vec<String>) -> Color {
        Color { data: ColorData::Custom {source, arguments} }
    }

    pub fn modified(first: &Color, operation: ColorOperation) -> Color {
        Color {
            data: ColorData::Modified {
                color: Box::new(first.clone()),
                operation,
            }
        }
    }
}

impl From<(u8, u8, u8)> for Color {
    fn from(value: (u8, u8, u8)) -> Self {
        Self {
            data: ColorData::Rgba {
                r: value.0,
                g: value.1,
                b: value.2,
                a: 255
            }
        }
    }
}

impl From<(u8, u8, u8, f32)> for Color {
    fn from(value: (u8, u8, u8, f32)) -> Self {
        Self {
            data: ColorData::Rgba {
                r: value.0,
                g: value.1,
                b: value.2,
                a: (value.3 * 255.0) as u8
            }
        }
    }
}

impl From<(f32, f32, f32, f32)> for Color {
    fn from(value: (f32, f32, f32, f32)) -> Self {
        Self {
            data: ColorData::Rgba {
                r: (value.0 * 255.0) as u8,
                g: (value.1 * 255.0) as u8,
                b: (value.2 * 255.0) as u8,
                a: (value.3 * 255.0) as u8,
            }
        }
    }
}

impl From<Value> for Color {
    fn from(value: Value) -> Self {
        if let ValueData::Color(color) = value.data {
            color
        } else {
            Color::empty()
        }
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.data {
            ColorData::Empty => write!(f, "Color(Empty)"),
            ColorData::Rgba { r, g, b, a } => write!(f, "Color(R {}, G {}, B {}, A {}", r, g, b, a),
            ColorData::Custom { source, arguments } => write!(f, "Color(Custom source: {} arguments: {})", source, arguments.join(", ")),
            ColorData::Modified { color, operation } => write!(f, "Color({})", operation.display_string(color.as_ref())),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum Unit {
    #[default] Unknown,
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

impl Unit {
    pub fn parse(input: &str) -> Unit {
        match input {
            "px" => Unit::Px,
            "em" => Unit::Em,
            "rem" => Unit::Rem,
            "pt" => Unit::Pt,
            "%" => Unit::Percent,
            "deg" => Unit::Degrees,
            "rad" => Unit::Radians,
            "s" => Unit::Seconds,
            "ms" => Unit::Milliseconds,
            "mm"
            | "cm"
            | "Q"
            | "in"
            | "pc"
            | "vh"
            | "vw"
            | "lh"
            | "rlh"
            | "grad"
            | "turn" => Unit::Unsupported,
            _ => Unit::Unknown,
        }
    }
}

impl ToString for Unit {
    fn to_string(&self) -> String {
        match self {
            Unit::Unknown => String::from("Unknown"),
            Unit::Unsupported => String::from("Unsupported"),
            Unit::Number => String::new(),
            Unit::Px => String::from("px"),
            Unit::Em => String::from("em"),
            Unit::Rem => String::from("rem"),
            Unit::Pt => String::from("pt"),
            Unit::Percent => String::from("%"),
            Unit::Degrees => String::from("°"),
            Unit::Radians => String::from("rad"),
            Unit::Seconds => String::from("s"),
            Unit::Milliseconds => String::from("ms"),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Dimension {
    pub value: f32,
    pub unit: Unit,
}

impl Dimension {
    pub fn px(value: f32) -> Dimension {
        Dimension { value, unit: Unit::Px }
    }

    pub fn is_number(&self) -> bool {
        self.unit == Unit::Number
    }

    pub fn is_length(&self) -> bool {
        match self.unit {
            Unit::Px | Unit::Em | Unit::Rem | Unit::Pt => true,
            _ => false,
        }
    }

    pub fn is_percent(&self) -> bool {
        self.unit == Unit::Percent
    }

    pub fn is_angle(&self) -> bool {
        match self.unit {
            Unit::Degrees | Unit::Radians => true,
            _ => false
        }
    }
}

impl From<Value> for Dimension {
    fn from(value: Value) -> Self {
        if let ValueData::Dimension(dimension) = value.data {
            dimension
        } else {
            Dimension { value: 0.0, unit: Unit::Unknown }
        }
    }
}

impl std::fmt::Display for Dimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Dimension({} {})", self.value, self.unit.to_string())
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum ValueData {
    #[default] Empty,
    Dimension(Dimension),
    String(String),
    Color(Color),
    Image(String),
    Url(String),
    Integer(i32),
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Value {
    pub(crate) data: ValueData
}

impl Value {
    pub fn empty() -> Value {
        Value{data: ValueData::Empty}
    }

    pub fn new_url(url: &str) -> Value {
        Value{data: ValueData::Url(url.to_string())}
    }

    pub fn empty_ref() -> &'static Value {
        &Value{data: ValueData::Empty}
    }

    pub fn to_string(&self) -> String {
        if let ValueData::String(string) = &self.data {
            string.clone()
        } else {
            String::new()
        }
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Value{data: ValueData::String(value.to_string())}
    }
}

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Value{data: ValueData::Dimension(Dimension { value, unit: Unit::Number })}
    }
}

impl From<i32> for Value {
    fn from(value: i32) -> Self {
        Value{data: ValueData::Integer(value)}
    }
}

impl From<&Identifier> for Value {
    fn from(value: &Identifier) -> Self {
        let id: String = value.into();
        Value::from(id.as_str())
    }
}

impl From<Color> for Value {
    fn from(value: Color) -> Self {
        Value{data: ValueData::Color(value)}
    }
}

impl From<Dimension> for Value {
    fn from(value: Dimension) -> Self {
        Value{data: ValueData::Dimension(value)}
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.data {
            ValueData::Empty => write!(f, "Empty Value"),
            ValueData::Dimension(dimension) => write!(f, "{}", dimension),
            ValueData::String(string) => write!(f, "String({})", string),
            ValueData::Image(string) => write!(f, "Image({})", string),
            ValueData::Url(string) => write!(f, "Url({})", string),
            ValueData::Color(color) => write!(f, "{}", color),
            ValueData::Integer(value) => write!(f, "Integer({})", value),
        }
    }
}

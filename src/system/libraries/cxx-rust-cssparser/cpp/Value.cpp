// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#include "Value.h"

#include <format>

#include "ffi.rs.h"

using namespace std::string_literals;

namespace cssparser
{

Dimension::Dimension()
{
}

Dimension::Dimension(Unit unit, float value)
    : m_unit(unit)
    , m_value(value)
{
}

std::string Dimension::toString() const
{
    switch (m_unit) {
    case Unit::Unknown:
        return "Unknown unit"s;
    case Unit::Unsupported:
        return "Unsupported unit"s;
    case Unit::Number:
        return std::format("{}", m_value);
    case Unit::Px:
        return std::format("{} px", m_value);
    case Unit::Em:
        return std::format("{} em", m_value);
    case Unit::Rem:
        return std::format("{} rem", m_value);
    case Unit::Pt:
        return std::format("{} pt", m_value);
    case Unit::Percent:
        return std::format("{} %", m_value);
    case Unit::Degrees:
        return std::format("{}°", m_value);
    case Unit::Radians:
        return std::format("{} rad", m_value);
    case Unit::Seconds:
        return std::format("{} s", m_value);
    case Unit::Milliseconds:
        return std::format("{} ms", m_value);
    }

    return std::format("{} (Unknown unit)", m_value);
}

inline Dimension::Unit convertUnit(rust::Unit unit)
{
    switch (unit) {
    case rust::Unit::Unknown:
        return Dimension::Unit::Unknown;
    case rust::Unit::Unsupported:
        return Dimension::Unit::Unsupported;
    case rust::Unit::Number:
        return Dimension::Unit::Number;
    case rust::Unit::Px:
        return Dimension::Unit::Px;
    case rust::Unit::Em:
        return Dimension::Unit::Em;
    case rust::Unit::Rem:
        return Dimension::Unit::Rem;
    case rust::Unit::Pt:
        return Dimension::Unit::Pt;
    case rust::Unit::Percent:
        return Dimension::Unit::Percent;
    case rust::Unit::Degrees:
        return Dimension::Unit::Degrees;
    case rust::Unit::Radians:
        return Dimension::Unit::Radians;
    case rust::Unit::Seconds:
        return Dimension::Unit::Seconds;
    case rust::Unit::Milliseconds:
        return Dimension::Unit::Milliseconds;
    }

    assert(false && "Mismatch between unit types in C++ and Rust, update C++ code!");
    return Dimension::Unit::Unknown;
}

Dimension Dimension::fromRust(rust::Dimension rustData)
{
    return Dimension{convertUnit(rustData.unit), rustData.value};
}

Value::Value()
{
}

inline Value::Type convertType(rust::ValueType rustType)
{
    switch (rustType) {
    case rust::ValueType::Empty:
        return Value::Type::Empty;
    case rust::ValueType::Dimension:
        return Value::Type::Dimension;
    case rust::ValueType::String:
        return Value::Type::String;
    case rust::ValueType::Color:
        return Value::Type::Color;
    case rust::ValueType::Image:
        return Value::Type::Image;
    case rust::ValueType::Url:
        return Value::Type::Url;
    case rust::ValueType::Integer:
        return Value::Type::Integer;
    }

    assert(false && "Mismatch between value types in C++ and Rust, update C++ code!");
    return Value::Type::Empty;
}

inline std::string valueTypeToString(Value::Type type)
{
    switch (type) {
    case Value::Type::Empty:
        return "Empty"s;
    case Value::Type::Dimension:
        return "Dimension"s;
    case Value::Type::String:
        return "String"s;
    case Value::Type::Color:
        return "Color"s;
    case Value::Type::Image:
        return "Image"s;
    case Value::Type::Url:
        return "Url"s;
    case Value::Type::Integer:
        return "Integer"s;
    }

    return "Unknown"s;
}

std::string Value::toString() const
{
    std::string data;
    switch (m_type) {
    case Value::Type::Empty:
        data = "(Empty)";
        break;
    case Value::Type::Dimension:
        data = std::get<Dimension>(m_data).toString();
        break;
    case Value::Type::String:
    case Value::Type::Image:
    case Value::Type::Url:
        data = std::get<std::string>(m_data);
        break;
    case Value::Type::Color:
        data = std::get<Color::Color>(m_data).toString();
        break;
    case Value::Type::Integer:
        data = std::to_string(std::get<int>(m_data));
    }

    return std::format("Value(type: {}, data: {})", valueTypeToString(m_type), data);
}

Value Value::fromRust(const rust::Value &rustData)
{
    auto result = Value{};
    result.m_type = convertType(rustData.value_type());

    switch (rustData.value_type()) {
    case rust::ValueType::Empty:
        break;
    case rust::ValueType::Dimension:
        result.m_data = Dimension::fromRust(rustData.to_dimension());
        break;
    case rust::ValueType::String:
        result.m_data = std::string(rustData.to_string());
        break;
    case rust::ValueType::Image:
        result.m_data = std::string(rustData.to_image());
        break;
    case rust::ValueType::Url:
        result.m_data = std::string(rustData.to_url());
        break;
    case rust::ValueType::Color:
        result.m_data = Color::Color::fromRust(rustData.to_color());
        break;
    case rust::ValueType::Integer:
        result.m_data = rustData.to_integer();
        break;
    }

    return result;
}

}

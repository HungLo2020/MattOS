// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#pragma once

#include <format>
#include <sstream>

#include "Color.h"

#include "cssparser_export.h"

namespace cssparser
{
namespace rust
{
struct Dimension;
struct Url;
struct Value;
}

/*!
 * \class cssparser::Dimension
 * \inmodule cxx-rust-cssparser
 *
 * \brief A representation of a dimension, which is a value and a unit.
 */
class CSSPARSER_EXPORT Dimension
{
public:
    /*!
     * \enum cssparser::Dimension::Unit
     *
     * The type and unit of a dimension.
     *
     * These correspond to CSS units.
     *
     * \value Unknown
     *      The unit is unknown.
     * \value Unsupported
     *      The unit is a proper CSS unit but we currently do not support it in the parser.
     * \value Number
     *      A unitless number.
     * \value Px
     *      A length in number of pixels.
     * \value Em
     *      A length in number of Ems.
     *      Ems are based on font size and relative to the parent font size.
     * \value Rem
     *      A length in number of Rems.
     *      Rems are based on font size, but unlike Ems relative to the root.
     * \value Pt
     *      A length in Points.
     *      Points are a font measurement unit equal to 1/72".
     * \value Percent
     *      A percentage.
     * \value Degrees
     *      An angle in Degrees.
     * \value Radians
     *      An angle in Radians.
     * \value Seconds
     *      A length of time measured in seconds.
     * \value Milliseconds
     *      A length of time measured in milliseconds.
     */
    enum class Unit {
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
    };

    /*!
     * Default constructor.
     */
    Dimension();
    /*!
     * Constructor.
     *
     * Constructs a dimension with unit \a unit and value \a value.
     */
    Dimension(Unit unit, float value);
    /*!
     * Conversion operator to float. Returns the value of this Dimension.
     */
    inline operator float() const
    {
        return m_value;
    }
    /*!
     * Returns the unit of this Dimension.
     */
    inline Unit unit() const
    {
        return m_unit;
    }
    /*!
     * Returns the value of this Dimension.
     */
    inline float value() const
    {
        return m_value;
    }
    /*!
     * Returns a string representation of this dimension.
     */
    std::string toString() const;

    // Internal: Convert from a rust Dimension to C++ Dimension.
    static Dimension fromRust(rust::Dimension rustData);

private:
    Unit m_unit = Unit::Unknown;
    float m_value = 0.0;
};

/*!
 * \class cssparser::Value
 * \inmodule cxx-rust-cssparser
 *
 * \brief A representation of a single value that can have several types.
 */
class CSSPARSER_EXPORT Value
{
public:
    /*!
     * \enum cssparser::Value::Type
     *
     * The type of value.
     *
     * \value Empty
     *      An empty value.
     * \value Dimension
     *      A Dimension.
     * \value String
     *      A string.
     * \value Color
     *      A Color.
     * \value Image
     *      An image, represented as a string path.
     * \value Url
     *      A URL, represented as a string.
     * \value Integer
     *      An integer.
     */
    enum class Type {
        Empty,
        Dimension,
        String,
        Color,
        Image,
        Url,
        Integer,
    };

    /*!
     * Default constructor.
     */
    Value();
    /*!
     * Constructor.
     *
     * Constructs a new value with type \a type and \a data as data.
     */
    template<typename T>
    Value(Type type, const T &data)
        : m_type(type)
        , m_data(data)
    {
    }
    /*!
     * Conversion operator to T.
     *
     * Returns the T stored in this value.
     */
    template<typename T>
    inline operator T() const
    {
        return std::get<T>(m_data);
    }
    /*!
     * Returns the T stored in this value.
     */
    template<typename T>
    inline T get() const
    {
        return std::get<T>(m_data);
    }
    /*!
     * Returns the type of this value.
     */
    inline Type type() const
    {
        return m_type;
    }
    /*!
     * Returns a string representation of this value.
     */
    std::string toString() const;

    // Internal: Convert from a rust Value to a C++ Value.
    static Value fromRust(const rust::Value &rustData);

private:
    Type m_type = Type::Empty;
    std::variant<std::nullopt_t, Dimension, std::string, Color::Color, int> m_data = std::nullopt;
};

}

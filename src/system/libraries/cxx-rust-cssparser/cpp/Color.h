// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <variant>
#include <vector>

#include "cssparser_export.h"

namespace rust
{
// Note that this namespace is an implementation detail, but without it, we
// cannot forward-declare the right type for Box<Color>.
inline namespace cxxbridge1
{
template<typename T>
struct Box;
}
}

namespace cssparser
{
namespace rust
{
struct Rgba;
struct CustomColor;
struct SetColorOperationValues;
struct MixColorOperationValues;
struct ModifiedColor;
struct Color;
}

namespace Color
{
class Color;

/*!
 * \namespace cssparser::Color
 *
 * Contains color-specific types.
 */

/*!
 * \class cssparser::Color::RgbaData
 *
 * \brief A color data class representing a color as an RGBA value.
 */
class CSSPARSER_EXPORT RgbaData
{
public:
    RgbaData();
    RgbaData(uint8_t r, uint8_t g, uint8_t b, uint8_t a);

    inline uint8_t r() const
    {
        return m_r;
    }

    inline uint8_t g() const
    {
        return m_g;
    }

    inline uint8_t b() const
    {
        return m_b;
    }

    inline uint8_t a() const
    {
        return m_a;
    }

    std::string toString() const;

    static RgbaData fromRust(rust::Rgba rustData);

private:
    uint8_t m_r = 0;
    uint8_t m_g = 0;
    uint8_t m_b = 0;
    uint8_t m_a = 0;
};

/*!
 * \class cssparser::Color::CustomColorData
 *
 * \brief A color data class representing a color as a name of a source and a list of arguments.
 */
class CSSPARSER_EXPORT CustomColorData
{
public:
    CustomColorData();
    CustomColorData(const std::string &source, const std::vector<std::string> &args);

    inline std::string source() const
    {
        return m_source;
    }

    inline std::vector<std::string> arguments() const
    {
        return m_arguments;
    }

    std::string toString() const;

    static CustomColorData fromRust(const rust::CustomColor &rustData);

private:
    std::string m_source;
    std::vector<std::string> m_arguments;
};

/*!
 * \class cssparser::Color::MixOperationData
 *
 * \brief The data required for a "mix" color operation.
 */
class CSSPARSER_EXPORT MixOperationData
{
public:
    MixOperationData();
    MixOperationData(const std::shared_ptr<Color> &other, float amount);

    inline std::shared_ptr<Color> other() const
    {
        return m_other;
    }

    inline float amount() const
    {
        return m_amount;
    }

    std::string toString() const;

    static MixOperationData fromRust(const rust::MixColorOperationValues &rustData);

private:
    std::shared_ptr<Color> m_other;
    float m_amount = 0.0;
};

/*!
 * \class cssparser::Color::SetOperationData
 *
 * \brief The data required for a "set" color operation.
 */
class CSSPARSER_EXPORT SetOperationData
{
public:
    SetOperationData();
    SetOperationData(std::optional<uint8_t> r, std::optional<uint8_t> g, std::optional<uint8_t> b, std::optional<uint8_t> a);

    inline std::optional<uint8_t> r() const
    {
        return m_r;
    }

    inline std::optional<uint8_t> g() const
    {
        return m_g;
    }

    inline std::optional<uint8_t> b() const
    {
        return m_b;
    }

    inline std::optional<uint8_t> a() const
    {
        return m_a;
    }

    std::string toString() const;

    static SetOperationData fromRust(const rust::SetColorOperationValues &rustData);

private:
    std::optional<uint8_t> m_r;
    std::optional<uint8_t> m_g;
    std::optional<uint8_t> m_b;
    std::optional<uint8_t> m_a;
};

/*!
 * \class cssparser::Color::ModifiedColorData
 *
 * \brief A color data class representing a color as a modification of another color.
 */
class CSSPARSER_EXPORT ModifiedColorData
{
public:
    enum class Operation : uint8_t {
        Unknown,
        Set,
        Add,
        Subtract,
        Multiply,
        Mix,
    };

    ModifiedColorData();

    template<typename T>
    ModifiedColorData(Operation operation, const std::shared_ptr<Color> &color, const T &data)
        : m_operation(operation)
        , m_color(color)
        , m_data(data)
    {
    }

    inline Operation operation() const
    {
        return m_operation;
    }

    inline std::shared_ptr<Color> color() const
    {
        return m_color;
    }

    template<typename T>
    inline T get() const
    {
        return std::get<T>(m_data);
    }

    std::string toString() const;

    static ModifiedColorData fromRust(const rust::ModifiedColor &rustData);

private:
    Operation m_operation = Operation::Unknown;
    std::shared_ptr<Color> m_color;
    std::variant<std::nullopt_t, std::shared_ptr<Color>, MixOperationData, SetOperationData> m_data = std::nullopt;
};

/*!
 * \class cssparser::Color::Color
 *
 * \brief A color that can be represented in various ways.
 */
class CSSPARSER_EXPORT Color
{
public:
    enum class Type : uint8_t {
        Empty,
        Rgba,
        Custom,
        Modified,
    };

    Color();

    template<typename T>
        requires std::is_same_v<T, RgbaData> || std::is_same_v<T, CustomColorData> || std::is_same_v<T, ModifiedColorData>
    Color(Type type, const T &data)
        : m_type(type)
        , m_data(data)
    {
    }
    /*!
     * Returns the type of color.
     */
    inline Type type() const
    {
        return m_type;
    }
    /*!
     * Returns the data of a specific type for this color.
     */
    template<typename T>
    inline T get() const
    {
        return std::get<T>(m_data);
    }
    /*!
     * Returns a string representation of this Color.
     */
    std::string toString() const;

    // Internal. Converts a rust Color to a C++ Color.
    static Color fromRust(const ::rust::cxxbridge1::Box<rust::Color> &color);

private:
    Type m_type = Type::Empty;
    std::variant<std::nullopt_t, RgbaData, CustomColorData, ModifiedColorData> m_data = std::nullopt;
};

}
}

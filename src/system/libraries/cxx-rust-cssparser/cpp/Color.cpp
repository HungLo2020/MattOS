// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#include "Color.h"

#include <format>

#include "ffi.rs.h"

using namespace std::string_literals;

namespace cssparser
{
namespace Color
{

RgbaData::RgbaData()
{
}

RgbaData::RgbaData(uint8_t r, uint8_t g, uint8_t b, uint8_t a)
    : m_r(r)
    , m_g(g)
    , m_b(b)
    , m_a(a)
{
}

std::string RgbaData::toString() const
{
    return std::format("RGBA({}, {}, {}, {})", m_r, m_g, m_r, m_a);
}

RgbaData RgbaData::fromRust(rust::Rgba rustData)
{
    return RgbaData{rustData.r, rustData.g, rustData.b, rustData.a};
}

CustomColorData::CustomColorData()
{
}

CustomColorData::CustomColorData(const std::string &source, const std::vector<std::string> &args)
    : m_source(source)
    , m_arguments(args)
{
}

std::string CustomColorData::toString() const
{
    auto args = std::string{};

    for (const auto &arg : m_arguments) {
        if (!args.empty()) {
            args += ", ";
        }
        args += arg;
    }

    return std::vformat("CustomColor(source: {}, arguments: {})"s, std::make_format_args(m_source, args));
}

CustomColorData CustomColorData::fromRust(const rust::CustomColor &rustData)
{
    std::vector<std::string> arguments;
    std::ranges::transform(rustData.arguments, std::back_inserter(arguments), [](auto arg) {
        return std::string(arg);
    });

    return CustomColorData{std::string(rustData.source), arguments};
}

MixOperationData::MixOperationData()
{
}

MixOperationData::MixOperationData(const std::shared_ptr<Color> &other, float amount)
    : m_other(other)
    , m_amount(amount)
{
}

std::string MixOperationData::toString() const
{
    return std::format("MixOperationData(other: {}, amount: {})", m_other->toString(), m_amount);
}

MixOperationData MixOperationData::fromRust(const rust::MixColorOperationValues &rustData)
{
    return MixOperationData{std::make_shared<Color>(Color::fromRust(rustData.other)), rustData.amount};
}

SetOperationData::SetOperationData()
{
}

SetOperationData::SetOperationData(std::optional<uint8_t> r, std::optional<uint8_t> g, std::optional<uint8_t> b, std::optional<uint8_t> a)
    : m_r(r)
    , m_g(g)
    , m_b(b)
    , m_a(a)
{
}

std::string SetOperationData::toString() const
{
    return std::format("SetOperationData(r: {}, g: {}, b: {}, a: {})", m_r.value_or(-1), m_g.value_or(-1), m_b.value_or(-1), m_a.value_or(-1));
}

SetOperationData SetOperationData::fromRust(const rust::SetColorOperationValues &rustData)
{
    auto result = SetOperationData{};
    if (rustData.r < 0) {
        result.m_r = std::nullopt;
    } else {
        result.m_r = uint8_t(rustData.r);
    }
    if (rustData.g < 0) {
        result.m_g = std::nullopt;
    } else {
        result.m_g = uint8_t(rustData.g);
    }
    if (rustData.b < 0) {
        result.m_b = std::nullopt;
    } else {
        result.m_b = uint8_t(rustData.b);
    }
    if (rustData.a < 0) {
        result.m_a = std::nullopt;
    } else {
        result.m_a = uint8_t(rustData.a);
    }
    return result;
}

ModifiedColorData::ModifiedColorData()
{
}

std::string ModifiedColorData::toString() const
{
    const auto color = m_color->toString();

    switch (m_operation) {
    case Operation::Add: {
        const auto add = std::get<std::shared_ptr<Color>>(m_data)->toString();
        return std::vformat("ModifiedColor(color: {}, operation: add, data: {})"s, std::make_format_args(color, add));
    }
    case Operation::Subtract: {
        const auto sub = std::get<std::shared_ptr<Color>>(m_data)->toString();
        return std::vformat("ModifiedColor(color: {}, operation: subtract, data: {})"s, std::make_format_args(color, sub));
    }
    case Operation::Multiply: {
        const auto mul = std::get<std::shared_ptr<Color>>(m_data)->toString();
        return std::vformat("ModifiedColor(color: {}, operation: multiply, data: {})"s, std::make_format_args(color, mul));
    }
    case Operation::Set: {
        const auto data = std::get<SetOperationData>(m_data).toString();
        return std::vformat("ModifiedColor(color: {}, operation: set, data: {})"s, std::make_format_args(color, data));
    }
    case Operation::Mix: {
        const auto data = std::get<MixOperationData>(m_data).toString();
        return std::vformat("ModifiedColor(color: {}, operation: mix, data: {})", std::make_format_args(color, data));
    }
    default:
        return "ModifiedColor(unknown operation)"s;
    }
}

inline ModifiedColorData::Operation convertOperation(rust::ColorOperationType type)
{
    switch (type) {
    case rust::ColorOperationType::Add:
        return ModifiedColorData::Operation::Add;
    case rust::ColorOperationType::Subtract:
        return ModifiedColorData::Operation::Subtract;
    case rust::ColorOperationType::Multiply:
        return ModifiedColorData::Operation::Multiply;
    case rust::ColorOperationType::Set:
        return ModifiedColorData::Operation::Set;
    case rust::ColorOperationType::Mix:
        return ModifiedColorData::Operation::Mix;
    }

    assert(false && "Mismatch between ModifiedColor operations in C++ and Rust, update C++ code!");
    return ModifiedColorData::Operation::Unknown;
}

ModifiedColorData ModifiedColorData::fromRust(const rust::ModifiedColor &rustData)
{
    auto result = ModifiedColorData{};
    result.m_color = std::make_shared<Color>(Color::fromRust(rustData.color));
    result.m_operation = convertOperation(rustData.operation_type());

    switch (rustData.operation_type()) {
    case rust::ColorOperationType::Add:
    case rust::ColorOperationType::Subtract:
    case rust::ColorOperationType::Multiply:
        result.m_data = std::make_shared<Color>(Color::fromRust(rustData.color_value()));
        break;
    case rust::ColorOperationType::Set: {
        const auto setData = rustData.set_values();
        result.m_data = SetOperationData{
            setData.r < 0 ? std::nullopt : std::optional<uint8_t>(setData.r),
            setData.g < 0 ? std::nullopt : std::optional<uint8_t>(setData.g),
            setData.b < 0 ? std::nullopt : std::optional<uint8_t>(setData.b),
            setData.a < 0 ? std::nullopt : std::optional<uint8_t>(setData.a),
        };
        break;
    }
    case rust::ColorOperationType::Mix: {
        const auto mixData = rustData.mix_values();
        result.m_data = MixOperationData{std::make_shared<Color>(Color::fromRust(mixData.other)), mixData.amount};
        break;
    }
    }

    return result;
}

Color::Color()
{
}

std::string Color::toString() const
{
    switch (m_type) {
    case Type::Empty:
        return "Color(Empty)"s;
    case Type::Rgba:
        return std::get<RgbaData>(m_data).toString();
    case Type::Custom:
        return std::get<CustomColorData>(m_data).toString();
    case Type::Modified:
        return std::get<ModifiedColorData>(m_data).toString();
    default:
        return "Color(Unknown)"s;
    }
}

inline Color::Type convertType(rust::ColorType type)
{
    switch (type) {
    case rust::ColorType::Empty:
        return Color::Type::Empty;
    case rust::ColorType::Rgba:
        return Color::Type::Rgba;
    case rust::ColorType::Custom:
        return Color::Type::Custom;
    case rust::ColorType::Modified:
        return Color::Type::Modified;
    }

    assert(false && "Mismatch between Color types in C++ and Rust, update C++ code!");
    return Color::Type::Empty;
}

Color Color::fromRust(const ::rust::cxxbridge1::Box<rust::Color> &color)
{
    auto result = Color{};
    result.m_type = convertType(color->color_type());

    switch (color->color_type()) {
    case rust::ColorType::Empty:
        result.m_data = std::nullopt;
        break;
    case rust::ColorType::Rgba:
        result.m_data = RgbaData::fromRust(color->to_rgba());
        break;
    case rust::ColorType::Custom:
        result.m_data = CustomColorData::fromRust(color->to_custom());
        break;
    case rust::ColorType::Modified:
        result.m_data = ModifiedColorData::fromRust(color->to_modified());
        break;
    }

    return result;
}

}
}

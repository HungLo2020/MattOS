// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#include "Selector.h"

#include <format>

#include "ffi.rs.h"

using namespace std::string_literals;

namespace cssparser
{

AttributeMatch::AttributeMatch(const std::string &name, Operator op, const Value &value)
    : m_name(name)
    , m_operator(op)
    , m_value(value)
{
}

SelectorPart::SelectorPart()
{
}

SelectorPart::SelectorPart(Kind kind, const Value &value, std::optional<AttributeMatch> match)
    : m_kind(kind)
    , m_value(value)
    , m_attributeMatch(match)
{
}

std::string kindToString(SelectorPart::Kind kind)
{
    switch (kind) {
    case SelectorPart::Kind::Unknown:
        return "Unknown"s;
    case SelectorPart::Kind::AnyElement:
        return "AnyElement"s;
    case SelectorPart::Kind::Type:
        return "Type"s;
    case SelectorPart::Kind::Class:
        return "Class"s;
    case SelectorPart::Kind::Id:
        return "Id"s;
    case SelectorPart::Kind::Attribute:
        return "Attribute"s;
    case SelectorPart::Kind::RelativeParent:
        return "RelativeParent"s;
    case SelectorPart::Kind::PseudoClass:
        return "PseudoClass"s;
    case SelectorPart::Kind::DocumentRoot:
        return "DocumentRoot"s;
    case SelectorPart::Kind::Part:
        return "Part"s;
    case SelectorPart::Kind::DescendantCombinator:
        return "DescendantCombinator"s;
    case SelectorPart::Kind::ChildCombinator:
        return "ChildCombinator"s;
    case SelectorPart::Kind::CombinatorStart:
        return "CombinatorStart"s;
    case SelectorPart::Kind::PartCombinator:
        return "PartCombinator"s;
    }

    return "Unknown"s;
}

std::string SelectorPart::toString() const
{
    auto kind = kindToString(m_kind);
    switch (m_kind) {
    case SelectorPart::Kind::Unknown:
    case SelectorPart::Kind::AnyElement:
    case SelectorPart::Kind::DocumentRoot:
    case SelectorPart::Kind::DescendantCombinator:
    case SelectorPart::Kind::ChildCombinator:
    case SelectorPart::Kind::PartCombinator:
        return std::format("SelectorPart(type: {})", kind);
    default:
        return std::format("SelectorPart(type: {}, value: {})", kind, m_value.toString());
    }
}

inline SelectorPart::Kind convertKind(rust::SelectorKind rustKind)
{
    switch (rustKind) {
    case rust::SelectorKind::Unknown:
        return SelectorPart::Kind::Unknown;
    case rust::SelectorKind::AnyElement:
        return SelectorPart::Kind::AnyElement;
    case rust::SelectorKind::Type:
        return SelectorPart::Kind::Type;
    case rust::SelectorKind::Class:
        return SelectorPart::Kind::Class;
    case rust::SelectorKind::Id:
        return SelectorPart::Kind::Id;
    case rust::SelectorKind::PseudoClass:
        return SelectorPart::Kind::PseudoClass;
    case rust::SelectorKind::Attribute:
        return SelectorPart::Kind::Attribute;
    case rust::SelectorKind::RelativeParent:
        return SelectorPart::Kind::RelativeParent;
    case rust::SelectorKind::DocumentRoot:
        return SelectorPart::Kind::DocumentRoot;
    case rust::SelectorKind::Part:
        return SelectorPart::Kind::Part;
    case rust::SelectorKind::DescendantCombinator:
        return SelectorPart::Kind::DescendantCombinator;
    case rust::SelectorKind::ChildCombinator:
        return SelectorPart::Kind::ChildCombinator;
    case rust::SelectorKind::PartCombinator:
        return SelectorPart::Kind::PartCombinator;
    }

    assert(false && "Mismatch between SelectorPart kinds in C++ and Rust, update C++ code!");
    return SelectorPart::Kind::Unknown;
}

inline AttributeMatch::Operator convertMatchOperator(rust::AttributeOperator op)
{
    switch (op) {
    case rust::AttributeOperator::None:
        return AttributeMatch::Operator::None;
    case rust::AttributeOperator::Exists:
        return AttributeMatch::Operator::Exists;
    case rust::AttributeOperator::Equals:
        return AttributeMatch::Operator::Equals;
    case rust::AttributeOperator::Includes:
        return AttributeMatch::Operator::Includes;
    case rust::AttributeOperator::Prefixed:
        return AttributeMatch::Operator::Prefixed;
    case rust::AttributeOperator::Suffixed:
        return AttributeMatch::Operator::Suffixed;
    case rust::AttributeOperator::Substring:
        return AttributeMatch::Operator::Substring;
    case rust::AttributeOperator::DashMatch:
        return AttributeMatch::Operator::DashMatch;
    }

    assert(false && "Mismatch between Attribute match operators in C++ and Rust, update C++ code!");
    return AttributeMatch::Operator::None;
}

SelectorPart SelectorPart::fromRust(const rust::SelectorPart &rustData)
{
    auto result = SelectorPart{};

    result.m_kind = convertKind(rustData.kind());
    result.m_value = Value::fromRust(rustData.value());

    if (rustData.attribute_operator() != rust::AttributeOperator::None) {
        result.m_attributeMatch = AttributeMatch{std::string(rustData.attribute_name()),
                                                 convertMatchOperator(rustData.attribute_operator()),
                                                 Value::fromRust(rustData.attribute_value())};
    }

    return result;
}

Selector::Selector()
{
}

Selector::Selector(const std::vector<SelectorPart> &parts)
    : m_parts(parts)
{
}

std::string Selector::toString() const
{
    std::string parts;

    for (const auto &part : m_parts) {
        if (!parts.empty()) {
            parts += " "s;
        }

        parts += part.toString();
    }

    return std::format("Selector(parts: {})", parts);
}

Selector Selector::fromRust(const rust::Selector &rustData)
{
    auto result = Selector();

    std::ranges::transform(rustData.parts(), std::back_inserter(result.m_parts), [](const auto &part) {
        return SelectorPart::fromRust(part);
    });

    return result;
}

}

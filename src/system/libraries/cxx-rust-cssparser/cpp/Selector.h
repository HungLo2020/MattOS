// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

#pragma once

#include <span>

#include "Value.h"

#include "cssparser_export.h"

namespace cssparser
{
namespace rust
{
struct AttributeMatch;
struct SelectorPart;
struct Selector;
}

/*!
 * \class cssparser::AttributeMatch
 *
 * \brief The data required for an attribute matching selector.
 */
class CSSPARSER_EXPORT AttributeMatch
{
public:
    enum class Operator : uint8_t {
        None,
        Exists,
        Equals,
        Includes,
        Prefixed,
        Suffixed,
        Substring,
        DashMatch,
    };

    AttributeMatch(const std::string &name, Operator op, const Value &value);

    inline std::string name() const
    {
        return m_name;
    }

    inline Operator op() const
    {
        return m_operator;
    }

    inline Value value() const
    {
        return m_value;
    }

private:
    std::string m_name;
    Operator m_operator;
    Value m_value;
};

/*!
 * \class cssparser::SelectorPart
 *
 * \brief One part of a complete selector.
 */
class CSSPARSER_EXPORT SelectorPart
{
public:
    enum class Kind : uint8_t {
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

        // Special value to mark the start of combinator selectors
        CombinatorStart,

        DescendantCombinator,
        ChildCombinator,
        PartCombinator,
    };

    SelectorPart();
    SelectorPart(Kind kind, const Value &value, std::optional<AttributeMatch> match = std::nullopt);

    inline bool isCombinator() const
    {
        return int(m_kind) > int(Kind::CombinatorStart);
    }

    inline Kind kind() const
    {
        return m_kind;
    }

    inline Value value() const
    {
        return m_value;
    }

    inline std::optional<AttributeMatch> attributeMatch() const
    {
        return m_attributeMatch;
    }

    std::string toString() const;

    static SelectorPart fromRust(const rust::SelectorPart &rustData);

private:
    Kind m_kind = Kind::Unknown;
    Value m_value;
    std::optional<AttributeMatch> m_attributeMatch;
};

/*!
 * \class cssparser::Selector
 *
 * \brief A selector determines which elements a Rule applies to.
 */
class CSSPARSER_EXPORT Selector
{
public:
    Selector();
    Selector(const std::vector<SelectorPart> &parts);

    inline std::span<const SelectorPart> parts() const
    {
        return std::span<const SelectorPart>(m_parts.cbegin(), m_parts.cend());
    }

    std::string toString() const;

    static Selector fromRust(const rust::Selector &rustData);

private:
    std::vector<SelectorPart> m_parts;
};

}

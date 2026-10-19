// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

#pragma once

#include <filesystem>

#include "Selector.h"

#include "cssparser_export.h"

namespace cssparser
{
namespace rust
{
struct Property;
struct StyleRule;
}

/*!
 * \class cssparser::Property
 * \inmodule cxx-rust-cssparser
 *
 * \brief A property of a CSS Rule.
 */
class CSSPARSER_EXPORT Property
{
public:
    /*!
     * Constructor.
     *
     * Constructs a new Property with  \a name as name and \a values as values.
     */
    Property(const std::string &name, const std::vector<Value> &values);
    /*!
     * Returns the name of this Property.
     */
    inline std::string name() const
    {
        return m_name;
    }
    /*!
     * Returns a view on the values of this Property.
     */
    inline std::span<const Value> values() const
    {
        return std::span<const Value>(m_values.cbegin(), m_values.cend());
    }
    /*!
     * Returns the value at \a index as type T.
     */
    template<typename T>
    inline T value(std::size_t index = 0) const
    {
        return m_values.at(index).get<T>();
    }
    /*!
     * Returns the value at \a index.
     */
    inline Value value(std::size_t index = 0) const
    {
        return m_values.at(index);
    }

    // Internal. Convert from a rust Property to a C++ Property.
    static Property fromRust(const rust::Property &rustData);

private:
    std::string m_name;
    std::vector<Value> m_values;
};

/*!
 * \class cssparser::Rule
 * \inmodule cxx-rust-cssparser
 *
 * \brief A single styling rule.
 */
class CSSPARSER_EXPORT Rule
{
public:
    /*!
     * Default constructor.
     */
    Rule();
    /*!
     * Constructor.
     *
     * Constructs a Rule with \a selector as selector and \a properties as properties.
     */
    Rule(const Selector &selector, const std::vector<Property> &properties);
    /*!
     * Returns the selector of this Rule.
     */
    inline Selector selector() const
    {
        return m_selector;
    }
    /*!
     * Returns the properties associated with this Rule.
     */
    inline std::span<const Property> properties() const
    {
        return std::span<const Property>(m_properties.cbegin(), m_properties.cend());
    }

    // Internal. Convert from a rust StyleRule to a C++ Rule.
    static Rule fromRust(const rust::StyleRule &rustData);

private:
    Selector m_selector;
    std::vector<Property> m_properties;
};

/*!
 * \inmodule cxx-rust-cssparser
 *
 * \brief A struct describing an error that happend during parsing.
 */
struct CSSPARSER_EXPORT Error {
    std::string file;
    uint32_t line = 0;
    uint32_t column = 0;
    std::string message;
};

/*!
 * \inmodule cxx-rust-cssparser
 *
 * \brief A collection of style rules.
 *
 */
class CSSPARSER_EXPORT StyleSheet
{
public:
    /*!
     * Default constructor.
     */
    StyleSheet(const std::filesystem::path &path);
    ~StyleSheet();

    /*!
     * A view of the list of rules contained in this StyleSheet.
     */
    std::span<const Rule> rules() const;
    /*!
     * A view of the list of errors generated when parsing this StyleSheet.
     */
    std::span<const Error> errors() const;
    /*!
     * A view of the list of files that were parsed by this StyleSheet.
     *
     * This includes files that were imported using \c{@import} in CSS.
     */
    std::span<const std::filesystem::path> paths() const;
    /*!
     * Parse a CSS file and add all rules to this StyleSheet.
     *
     * This will read the file from \a file_name and parse it. After this, all
     * sucessfully parsed rules are available through rules(). Any errors
     * encountered while parsing will be available through errors().
     *
     * \note Multiple calls will append to the internal list of rules and
     * errors.
     */
    void parse();
    /*!
     * Parse a string containing CSS and add all rules to this StyleSheet.
     *
     * This will parse the CSS in \a data. After this, all sucessfully parsed
     * rules are available through rules(). Any errors encountered while parsing
     * will be available through errors().
     *
     * \a origin is used as the file path that errors are reported from.
     *
     * \note Multiple calls will append to the internal list of rules and
     * errors.
     */
    void parseString(const std::string &data);

    void import(const std::filesystem::path &path);

private:
    struct Private;
    const std::unique_ptr<Private> d;
};

}

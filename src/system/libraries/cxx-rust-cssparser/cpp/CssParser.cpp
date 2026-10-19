// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2024 Arjen Hiemstra <ahiemstra@heimr.nl>

#include "CssParser.h"

#include <iostream>
#include <fstream>

#include "ffi.rs.h"

using namespace std::string_literals;

using namespace cssparser;

Property::Property(const std::string &name, const std::vector<Value> &values)
    : m_name(name)
    , m_values(values)
{
}

Property Property::fromRust(const rust::Property &rustData)
{
    std::vector<Value> values;
    for (const auto &rustValue : rustData.values()) {
        values.push_back(Value::fromRust(rustValue));
    }

    return Property{std::string(rustData.name()), values};
}

Rule::Rule()
{
}

Rule::Rule(const Selector &selector, const std::vector<Property> &properties)
    : m_selector(selector)
    , m_properties(properties)
{
}

Rule Rule::fromRust(const rust::StyleRule &rule)
{
    auto result = Rule{};
    result.m_selector = Selector::fromRust(rule.selector());

    for (const auto &property : rule.properties()) {
        result.m_properties.push_back(Property::fromRust(property));
    }

    return result;
}

struct StyleSheet::Private
{
    Private(std::filesystem::path path)
        : path(path)
        , stylesheet(rust::create_stylesheet(path.string()))
    {
    }

    void update();

    std::filesystem::path path;

    ::rust::Box<rust::StyleSheet> stylesheet;
    std::vector<Rule> rules;
    std::vector<Error> errors;
    std::vector<std::filesystem::path> paths;
};

StyleSheet::StyleSheet(const std::filesystem::path &path)
    : d(std::make_unique<Private>(path))
{
}

StyleSheet::~StyleSheet() = default;

std::span<const Rule> StyleSheet::rules() const
{
    return std::span<const Rule>(d->rules.cbegin(), d->rules.cend());
}

std::span<const Error> StyleSheet::errors() const
{
    return std::span<const Error>(d->errors.cbegin(), d->errors.cend());
}

std::span<const std::filesystem::path> StyleSheet::paths() const
{
    return std::span<const std::filesystem::path>(d->paths.cbegin(), d->paths.cend());
}

void StyleSheet::parse()
{
    try {
        d->stylesheet->parse();
    } catch (const std::exception &e) {
        d->errors.push_back(Error{
            .file = d->path,
            .line = 0,
            .column = 0,
            .message = e.what(),
        });

        return;
    }

    d->update();
}

void StyleSheet::parseString(const std::string &source)
{
    d->stylesheet->parse_string(source);
    d->update();
}

void cssparser::StyleSheet::import(const std::filesystem::path &path)
{
    d->stylesheet->import_file(path.string());
}

void StyleSheet::Private::update()
{
    rules.clear();
    for (const auto &entry : stylesheet->rules()) {
        auto rule = Rule::fromRust(entry);
        rules.push_back(rule);
    }

    errors.clear();
    for (const auto &entry : stylesheet->errors()) {
        errors.push_back(Error{
            .file = std::string(entry.file),
            .line = entry.line,
            .column = entry.column,
            .message = std::string(entry.message),
        });
    }

    paths.clear();
    for (const auto &entry : stylesheet->paths()) {
        paths.push_back(std::filesystem::path(std::string(entry)));
    }
}

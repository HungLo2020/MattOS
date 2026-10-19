// SPDX-License-Identifier: LGPL-2.1-only OR LGPL-3.0-only OR LicenseRef-KDE-Accepted-LGPL
// SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

#include <filesystem>
#include <fstream>
#include <iostream>

#include <getopt.h>

#include "CssParser.h"

using namespace cssparser;
using namespace std::string_literals;

std::string kind_to_string(SelectorPart::Kind kind)
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
    case SelectorPart::Kind::DescendantCombinator:
        return "DescendantCombinator";
    case SelectorPart::Kind::ChildCombinator:
        return "ChildCombinator";
    default:
        return "Unkown"s;
    }
}

std::string value_to_string(const Value &value)
{
    switch (value.type()) {
    case Value::Type::Empty:
        return "(Empty)"s;
    case Value::Type::Dimension:
        return value.get<Dimension>().toString();
    case Value::Type::String:
    case Value::Type::Image:
    case Value::Type::Url:
        return value.get<std::string>();
    case Value::Type::Color:
        return value.get<Color::Color>().toString();
    case Value::Type::Integer:
        return std::to_string(value.get<int>());
    }

    return std::string{};
}

std::string selector_part_to_string(const SelectorPart &part)
{
    auto kind = kind_to_string(part.kind());
    switch (part.kind()) {
    case SelectorPart::Kind::Unknown:
    case SelectorPart::Kind::AnyElement:
    case SelectorPart::Kind::DocumentRoot:
    case SelectorPart::Kind::DescendantCombinator:
    case SelectorPart::Kind::ChildCombinator:
        return kind;
    default:
        return kind + ": " + value_to_string(part.value());
    }
}

int main(int argc, char **argv)
{
    std::vector<std::filesystem::path> import_files;
    bool show_help = false;
    bool verbose = false;

    auto options = std::array{
        option{.name = "verbose", .has_arg = no_argument, .flag = nullptr, .val = 'v'},
        option{.name = "import", .has_arg = required_argument, .flag = nullptr, .val = 'i'},
        option{.name = "help", .has_arg = no_argument, .flag = nullptr, .val = 'h'},
    };

    int c = 0;
    int i = 0;
    while ((c = getopt_long(argc, argv, "", options.data(), &i)) != -1) {
        switch (c) {
        case 'v':
            verbose = true;
            break;
        case 'i':
            import_files.push_back(std::string(optarg));
            break;
        case 'h':
            show_help = true;
            break;
        default:
            std::cout << "Unrecognized option: " << c;
            show_help = true;
        }
    }

    if (optind >= argc && show_help == 0) {
        std::cerr << "A file path is required!" << std::endl;
        show_help = true;
    }

    if (show_help) {
        std::cout << "Usage: " << argv[0] << " [options] <filename>\n\n";
        std::cout << "Options:\n";
        std::cout << "--verbose             Print full structure of parsed data.\n";
        std::cout << "--import <filename>   Import <filename> before parsing the main file.\n";
        exit(1);
    }

    std::filesystem::path path(argv[optind]);

    StyleSheet sheet(path);

    for (const auto &file : import_files) {
        sheet.import(file);
    }

    sheet.parse();

    auto errors = sheet.errors();
    if (!errors.empty()) {
        std::cout << errors.size() << " errors:" << std::endl;

        for (const auto &error : errors) {
            std::cout << error.file << " line " << error.line << " column " << error.column << ": " << error.message << std::endl;
        }

        exit(2);
    }

    auto result = sheet.rules();
    std::cout << result.size() << " results:" << std::endl;

    for (const auto &entry : result) {
        std::cout << "Rule {\n";

        if (!verbose) {
            std::cout << "  selector:\n";
            const auto selector = entry.selector();
            for (const auto &part : selector.parts()) {
                std::cout << "    " << selector_part_to_string(part) << "\n";
            }
            std::cout << "\n";

            for (const auto &property : entry.properties()) {
                if (property.values().size() == 1) {
                    std::cout << "  " << property.name() << ": " << value_to_string(property.values().front()) << "\n";
                } else {
                    std::cout << "\n";
                    std::cout << "  " << property.name() << ":\n";
                    for (const auto &value : property.values()) {
                        std::cout << "    " << value_to_string(value) << "\n";
                    }
                }
            }
        } else {
            std::cout << "  Selector {\n";
            const auto selector = entry.selector();
            for (const auto &part : selector.parts()) {
                std::cout << "    " << part.toString() << "\n";
            }
            std::cout << "  }\n\n";

            for (const auto &property : entry.properties()) {
                if (property.values().size() == 1) {
                    std::cout << "  Property(name: " << property.name() << ", value: " << property.values().front().toString() << "\n";
                } else {
                    std::cout << "  Property {\n";
                    std::cout << "    name: " << property.name() << "\n";
                    std::cout << "    values:\n";
                    for (const auto &value : property.values()) {
                        std::cout << "    - " << value.toString() << "\n";
                    }
                    std::cout << "  }\n";
                }
            }
        }

        std::cout << "}\n\n";
    }
}

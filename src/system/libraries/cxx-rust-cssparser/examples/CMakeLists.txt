# SPDX-License-Identifier: BSD-2-Clause
# SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

add_executable(cxx-rust-cssparser-parse main.cpp)

target_link_libraries(cxx-rust-cssparser-parse cxx-rust-cssparser)

install(TARGETS cxx-rust-cssparser-parse DESTINATION ${CMAKE_INSTALL_BINDIR})

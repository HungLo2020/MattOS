# SPDX-License-Identifier: BSD-2-Clause
# SPDX-FileCopyrightText: 2025 Arjen Hiemstra <ahiemstra@heimr.nl>

corrosion_import_crate(MANIFEST_PATH Cargo.toml)
corrosion_set_env_vars(cxx_rust_cssparser_impl CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR}/output)

set_source_files_properties(
    ${CMAKE_CURRENT_BINARY_DIR}/output/cxxbridge/cxx-rust-cssparser-impl/src/ffi.rs.cc
    ${CMAKE_CURRENT_BINARY_DIR}/output/cxxbridge/cxx-rust-cssparser-impl/src/ffi.rs.h
    PROPERTIES GENERATED ON
)

add_library(cxx-rust-cssparser-impl-bridge STATIC)
target_sources(cxx-rust-cssparser-impl-bridge PRIVATE
    ${CMAKE_CURRENT_BINARY_DIR}/output/cxxbridge/cxx-rust-cssparser-impl/src/ffi.rs.cc
    ${CMAKE_CURRENT_BINARY_DIR}/output/cxxbridge/cxx-rust-cssparser-impl/src/ffi.rs.h
)

target_include_directories(cxx-rust-cssparser-impl-bridge PUBLIC
    ${CMAKE_CURRENT_BINARY_DIR}/output/cxxbridge/cxx-rust-cssparser-impl/src
)

set_target_properties(cxx-rust-cssparser-impl-bridge PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_compile_options(cxx-rust-cssparser-impl-bridge PRIVATE -Wno-maybe-uninitialized)
target_link_libraries(cxx-rust-cssparser-impl-bridge PRIVATE cxx_rust_cssparser_impl)

add_dependencies(cxx-rust-cssparser-impl-bridge cxx_rust_cssparser_impl)

if (BUILD_TESTING)
    add_test(
        NAME cargo-test
        COMMAND $<TARGET_FILE:Rust::Cargo> test --manifest-path=${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml
        WORKING_DIRECTORY ${CMAKE_BINARY_DIR}
    )
endif()

// SPDX-License-Identifier: BSD-2-Clause
// SPDX-FileCopyrightText: 2026 Arjen Hiemstra <ahiemstra@heimr.nl>

fn main() {
    cxx_build::bridge("src/ffi.rs")
        .std("c++20")
        .flag("-Wno-maybe-uninitialized")
        .compile("cxx-rust-cssparser-impl");
}

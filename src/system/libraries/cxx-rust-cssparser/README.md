# cxx-rust-cssparser

A C++ library for parsing CSS that uses the Rust cssparser crate internally.

## Getting Started

To build cxx-rust-cssparser you will need both a C++ and Rust compiler, as well
as [CMake] and [Corrosion]. The Rust code has several dependencies, these are
handled by Cargo at build time. Build the project like any other CMake project,
it will build the Rust parts while building the rest.

[CMake]: https://www.cmake.org
[Corrosion]: https://github.com/corrosion-rs/corrosion

## Overview

The goal of this library is not necessarily to implement a full-fledged
web-compatible CSS parser. Instead, it intends to give enough building blocks
to be able to implement such a parser, but also allows you to deviate from the
specification if needed.

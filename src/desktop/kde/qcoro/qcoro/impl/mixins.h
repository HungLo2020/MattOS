// SPDX-FileCopyrightText: 2025 Daniel Vrátil <dvratil@kde.org>
//
// SPDX-License-Identifier: MIT

#pragma once

namespace QCoro::detail {

template<typename T, typename Awaiter>
inline auto AwaitTransformMixin::await_transform(T &&value) {
    return Awaiter{std::forward<T>(value)};
}

template<Awaitable T>
inline auto && AwaitTransformMixin::await_transform(T &&awaitable) {
    return std::forward<T>(awaitable);
}

template<Awaitable T>
inline auto &AwaitTransformMixin::await_transform(T &awaitable) {
    return awaitable;
}

}
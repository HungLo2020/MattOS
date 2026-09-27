// SPDX-FileCopyrightText: 2025 Daniel Vrátil <dvratil@kde.org>
//
// SPDX-License-Identifier: MIT

#pragma once

namespace QCoro::detail {

template<typename T>
struct awaiter_type;

template<typename T>
using awaiter_type_t = typename awaiter_type<T>::type;

class AwaitTransformMixin {
public:
    //! Called by `co_await` to obtain an Awaitable for type \c T.
    /*!
     * When co_awaiting on a value of type \c T, the type \c T must an Awaitable. To allow
     * to co_await even on types that are not Awaitable (e.g. 3rd party types like QNetworkReply),
     * C++ allows promise_type to provide \c await_transform() function that can transform
     * the type \c T into an Awaitable. This is a very powerful mechanism in C++ coroutines.
     *
     * For types \c T for which there is no valid await_transform() overload, the C++ attempts
     * to use those types directly as Awaitables. This is a perfectly valid scenario in cases
     * when co_awaiting a type that implements the neccessary Awaitable interface.
     *
     * In our implementation, the await_transform() is overloaded only for Qt types for which
     * a specialiation of the \c QCoro::detail::awaiter_type template class exists. The
     * specialization returns type of the Awaiter for the given type \c T.
     */
    template<typename T, typename Awaiter = QCoro::detail::awaiter_type_t<std::remove_cvref_t<T>>>
    auto await_transform(T &&value);

    //! If the type T is already an awaitable (including Task or LazyTask), then just forward it as it is.
    template<Awaitable T>
    auto && await_transform(T &&awaitable);

    //! \copydoc template<Awaitable T> QCoro::TaskPromiseBase::await_transform(T &&)
    template<Awaitable T>
    auto &await_transform(T &awaitable);
};

} // namespace QCoro::detail

#include "impl/mixins.h"
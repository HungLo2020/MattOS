set(dbustest_SRCS
    main.cpp
)

add_executable(dbustest-coro ${dbustest_SRCS})
target_link_libraries(dbustest-coro
    QCoro${QT_VERSION_MAJOR}DBus
    examples-dbus-common
    Threads::Threads
    Qt${QT_VERSION_MAJOR}::Core
    Qt${QT_VERSION_MAJOR}::DBus
)
set_target_defaults(dbustest-coro)


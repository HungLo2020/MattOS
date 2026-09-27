add_executable(await-sync-string await-sync-string.cpp)
target_link_libraries(await-sync-string QCoro${QT_VERSION_MAJOR}::Coro)
set_target_defaults(await-sync-string)

add_executable(await-async-string await-async-string.cpp)
target_link_libraries(await-async-string QCoro${QT_VERSION_MAJOR}::Coro Qt${QT_VERSION_MAJOR}::Core)
set_target_defaults(await-async-string)


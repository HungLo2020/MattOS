cmake_minimum_required(VERSION 3.16)
project(MattOSDesktopSmoke LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
# This plain C++ probe loads QML plugins dynamically through QQmlEngine. It
# needs neither QML generators nor CMake imports for static QML plugins.
set(QT_ALLOW_MISSING_TOOLS_PACKAGES TRUE)
set(QT_SKIP_AUTO_QML_PLUGIN_INCLUSION TRUE)
find_package(Qt6 REQUIRED COMPONENTS Widgets Qml)
find_package(KF6ColorScheme REQUIRED)
add_executable(desktop-smoke desktop-smoke.cpp)
target_link_libraries(desktop-smoke PRIVATE Qt6::Widgets Qt6::Qml KF6::ColorScheme)

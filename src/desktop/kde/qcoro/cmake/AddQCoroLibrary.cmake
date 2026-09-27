# SPDX-FileCopyrightText: 2022 Daniel Vrátil <dvratil@kde.org>
#
# SPDX-License-Identifier: MIT

include(GenerateHeaders)
include(GenerateExportHeader)
include(GenerateModuleConfigFile)
include(ECMGeneratePriFile)

function(set_target_defaults target_name)
    set(DEFAULT_QT_DEFINITIONS QT_NO_CAST_FROM_ASCII QT_NO_CAST_TO_ASCII QT_NO_URL_CAST_FROM_STRING QT_NO_CAST_FROM_BYTEARRAY QT_USE_STRINGBUILDER QT_NO_NARROWING_CONVERSIONS_IN_CONNECT QT_NO_KEYWORDS QT_NO_FOREACH)

    get_target_property(target_type ${target_name} TYPE)
    if (target_type STREQUAL "INTERFACE_LIBRARY")
        # We can't set compile definitions for interface libraries as that would leak into user code
        return()
    endif()

    target_compile_definitions(${target_name} PRIVATE ${DEFAULT_QT_DEFINITIONS})
    
    if (NOT WIN32)
        # strict iterators on MSVC only work when Qt itself is also built with them,
        # which is not usually the case. Otherwise there are linking issues.
        target_compile_definitions(${target_name} PRIVATE QT_STRICT_ITERATORS)
    endif()

    string(TOLOWER "${CMAKE_BUILD_TYPE}" build_type_lowercase)
    if ("${build_type_lowercase}" STREQUAL "debug")
        if (MSVC)
            target_compile_options(${target_name} PRIVATE /W4 /WX)
            # Disable warning C5054: "operator '&': deprecated between enumerations of different types" caused by QtWidgets/qsizepolicy.h
            # Disable warning C4127: "conditional expression is constant" caused by QtCore/qiterable.h
            target_compile_options(${target_name} PRIVATE /wd5054 /wd4127)
            if ("${QT_VERSION_MAJOR}" STREQUAL "6" AND "${Qt6_VERSION}" VERSION_GREATER_EQUAL "6.4.0" AND "${Qt6_VERSION}" VERSION_LESS "6.5.3")
                # Disable warning C4702: "unreachable code" caused by QtTest/qtestcase.h - fixed in Qt 6.5.3
                target_compile_options(${target_name} PRIVATE /wd4702)
            endif()
        else()
            target_compile_options(${target_name} PRIVATE -Wall -Wextra -Werror -pedantic -Wno-language-extension-token)
        endif()
    endif()

endfunction()

function(add_qcoro_library)
    function(prefix_libraries)
        set(oneValueArgs PREFIX OUTPUT)
        set(multiValueArgs LIBRARIES)
        cmake_parse_arguments(prf "" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})

        set(_libs)
        foreach(libname ${prf_LIBRARIES})
            if ("${libname}" MATCHES "PUBLIC|PRIVATE|INTERFACE")
                list(APPEND _libs "${libname}")
            else()
                list(APPEND _libs "${prf_PREFIX}::${libname}")
            endif()
        endforeach()

        set(${prf_OUTPUT} ${_libs} PARENT_SCOPE)
    endfunction()

    function(process_qmake_deps)
        set(oneValueArgs PREFIX OUTPUT)
        set(multiValueArgs LIBRARIES)
        cmake_parse_arguments(pqd "" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})

        set(_libs_priv FALSE)
        set(_deps)
        foreach (dep ${pqd_LIBRARIES})
            if ("${dep}" MATCHES "PUBLIC|INTERFACE|public|interface")
                set(_libs_priv FALSE)
                continue()
            elseif ("${dep}" MATCHES "PRIVATE|private")
                set(_libs_priv TRUE)
                continue()
            endif()
            if (NOT _libs_priv)
                set(_deps "${_deps} ${pqd_PREFIX}${dep}")
            endif()
        endforeach()
        set(${pqd_OUTPUT} ${_deps} PARENT_SCOPE)
    endfunction()

    set(params INTERFACE NO_CMAKE_CONFIG)
    set(oneValueArgs NAME)
    set(multiValueArgs SOURCES CAMELCASE_HEADERS HEADERS QCORO_LINK_LIBRARIES QT_LINK_LIBRARIES)

    cmake_parse_arguments(LIB "${params}" "${oneValueArgs}" "${multiValueArgs}" ${ARGN})

    set(target_name "${QCORO_TARGET_PREFIX}${LIB_NAME}")
    string(TOLOWER "${target_name}" target_name_lowercase)
    set(target_interface)
    set(target_include_interface "PUBLIC")
    if (LIB_INTERFACE)
        set(target_interface "INTERFACE")
        set(target_include_interface "INTERFACE")
    endif()

    prefix_libraries(
        PREFIX ${QCORO_TARGET_PREFIX}
        LIBRARIES ${LIB_QCORO_LINK_LIBRARIES}
        OUTPUT qcoro_LIBS
    )

    prefix_libraries(
        PREFIX Qt${QT_VERSION_MAJOR}
        LIBRARIES ${LIB_QT_LINK_LIBRARIES}
        OUTPUT qt_LIBS
    )

    # TODO: How is it done in Qt?
    # We want to export target QCoro5::Network but so far we are exporting
    # QCoro5::QCoro5Network :shrug:
    add_library(${target_name} ${target_interface})
    add_library(${QCORO_TARGET_PREFIX}::${LIB_NAME} ALIAS ${target_name})
    if (LIB_SOURCES)
        target_sources(${target_name} PRIVATE ${LIB_SOURCES})
    endif()

    target_include_directories(
        ${target_name}
        ${target_include_interface} $<BUILD_INTERFACE:${QCORO_SOURCE_DIR}>
        ${target_include_interface} $<BUILD_INTERFACE:${QCORO_SOURCE_DIR}/qcoro>
        ${target_include_interface} $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}>
        ${target_include_interface} $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}>
        ${target_include_interface} $<BUILD_INTERFACE:${CMAKE_CURRENT_BINARY_DIR}/QCoro>
        ${target_include_interface} $<INSTALL_INTERFACE:${CMAKE_INSTALL_INCLUDEDIR}>
        ${target_include_interface} $<INSTALL_INTERFACE:${QCORO_INSTALL_INCLUDEDIR}>
        ${target_include_interface} $<INSTALL_INTERFACE:${QCORO_INSTALL_INCLUDEDIR}/qcoro>
        ${target_include_interface} $<INSTALL_INTERFACE:${QCORO_INSTALL_INCLUDEDIR}/QCoro>
    )

    target_link_libraries(${target_name} ${qcoro_LIBS})
    target_link_libraries(${target_name} ${qt_LIBS})

    if(NOT HAVE_CXX_ATOMICS_WITHOUT_LIB AND NOT LIB_INTERFACE)
        target_link_libraries(${target_name} PUBLIC atomic)
    endif()

    set_target_properties(
        ${target_name}
        PROPERTIES
        EXPORT_NAME ${LIB_NAME}
    )

    set_target_defaults(${target_name})
    
    if (NOT LIB_INTERFACE)
        set_target_properties(
            ${target_name}
            PROPERTIES
            WINDOWS_EXPORT_ALL_SYMBOLS 1
            VERSION ${qcoro_VERSION}
            SOVERSION ${qcoro_SOVERSION}
        )
        target_code_coverage(${target_name} AUTO)

    else()
        target_code_coverage(${target_name} AUTO INTERFACE)
    endif()


    generate_headers(
        camelcase_HEADERS
        HEADER_NAMES ${LIB_CAMELCASE_HEADERS}
        OUTPUT_DIR QCoro
        ORIGINAL_HEADERS_VAR source_HEADERS
    )

    if (NOT LIB_INTERFACE)
        string(TOUPPER "qcoro${LIB_NAME}" export_name)
        string(TOLOWER "${export_name}" export_file)
        generate_export_header(
            ${target_name}
            BASE_NAME ${export_name}
        )
    endif()

    if (NOT LIB_NO_CMAKE_CONFIG)
        generate_cmake_module_config_file(
            NAME ${LIB_NAME}
            TARGET_NAME ${target_name}
            QT_DEPENDENCIES ${LIB_QT_LINK_LIBRARIES}
            QCORO_DEPENDENCIES ${LIB_QCORO_LINK_LIBRARIES}
        )
    endif()

    string(TOLOWER "${LIB_QT_LINK_LIBRARIES}" lc_qt_link_libraries)
    process_qmake_deps(
        OUTPUT qmake_qt_deps
        LIBRARIES ${lc_qt_link_libraries}
    )

    process_qmake_deps(
        PREFIX QCoro
        OUTPUT qmake_qcoro_deps
        LIBRARIES ${LIB_QCORO_LINK_LIBRARIES}
    )

    set(egp_INTERFACE)
    if (LIB_INTERFACE)
        set(egp_INTERFACE "INTERFACE")
    endif()

    ecm_generate_pri_file(
        ${egp_INTERFACE}
        BASE_NAME QCoro${LIB_NAME}
        LIB_NAME ${target_name}
        VERSION ${qcoro_VERSION}
        INCLUDE_INSTALL_DIRS ${QCORO_INSTALL_INCLUDEDIR}/qcoro;${QCORO_INSTALL_INCLUDEDIR}/QCoro
        DEPS "${qmake_qt_deps} ${qmake_qcoro_deps}"
    )

    install(
        TARGETS ${target_name}
        EXPORT ${target_name}Targets
    )
    install(
        FILES ${source_HEADERS}
        DESTINATION ${QCORO_INSTALL_INCLUDEDIR}/qcoro/
        COMPONENT Devel
    )
    foreach(lib_header ${LIB_HEADERS})
        get_filename_component(header_prefix_dir ${lib_header} DIRECTORY)
        install(
            FILES ${lib_header}
            DESTINATION ${QCORO_INSTALL_INCLUDEDIR}/qcoro/${header_prefix_dir}
            COMPONENT Devel
        )
    endforeach()
    install(
        FILES ${camelcase_HEADERS}
        DESTINATION ${QCORO_INSTALL_INCLUDEDIR}/QCoro/
        COMPONENT Devel
    )
    if (NOT LIB_INTERFACE)
        install(FILES ${CMAKE_CURRENT_BINARY_DIR}/${export_file}_export.h
                DESTINATION ${QCORO_INSTALL_INCLUDEDIR}/qcoro
                COMPONENT Devel
        )
    endif()

    install(
        FILES "${CMAKE_CURRENT_BINARY_DIR}/${target_name}Config.cmake"
              "${CMAKE_CURRENT_BINARY_DIR}/${target_name}ConfigVersion.cmake"
        DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/${target_name}"
        COMPONENT Devel
    )
    install(
        EXPORT ${target_name}Targets
        DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/${target_name}"
        FILE "${target_name}Targets.cmake"
        NAMESPACE ${QCORO_TARGET_PREFIX}::
        COMPONENT Devel
    )
    install(
        FILES "${CMAKE_CURRENT_BINARY_DIR}/qt_QCoro${LIB_NAME}.pri"
        DESTINATION "${ECM_MKSPECS_INSTALL_DIR}"
        COMPONENT Devel
    )
endfunction()

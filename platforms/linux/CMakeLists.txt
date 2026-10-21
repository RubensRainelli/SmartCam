# Personal and Educational Free License (PEF License / PEFL)
# Version 1.0 - 2026-05-26
# Copyright (c) 2026 Rubens Rainelli
#
# Contact Details:
#   Email: rubens@rainelli.it
#   GitHub: https://github.com/RubensRainelli/
#   Tel: (+39) 342 811 5882
#
# Personal and educational free use permitted.
# Corporate, business or enterprise use prohibited without written authorization and fee.
# The full license can be found in the LICENSE file.

cmake_minimum_required(VERSION 3.20)
project(SmartCamLinux VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

if(NOT UNIX OR APPLE)
    message(WARNING "platforms/linux/CMakeLists.txt is designed for Linux (V4L2 + v4l2loopback).")
endif()

option(SMARTCAM_ENABLE_AVX512 "Enable AVX-512 compile flags alongside runtime CPUID dispatch" OFF)
option(SMARTCAM_ENABLE_TESTS "Build platform-agnostic C++ unit tests for common/engine" ON)

# Place all built .so libraries, static archives, and test executables in the variant build directory
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
foreach(cfg IN ITEMS RELEASE DEBUG RELWITHDEBINFO MINSIZEREL)
    set(CMAKE_RUNTIME_OUTPUT_DIRECTORY_${cfg} "${CMAKE_BINARY_DIR}")
    set(CMAKE_LIBRARY_OUTPUT_DIRECTORY_${cfg} "${CMAKE_BINARY_DIR}")
    set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY_${cfg} "${CMAKE_BINARY_DIR}")
endforeach()

enable_testing()

find_package(Threads REQUIRED)

# 1. Include platform-agnostic common engine (AVX2 / AVX-512 + GPU Compute Dispatcher)
add_subdirectory(
    ${CMAKE_CURRENT_SOURCE_DIR}/../../common/engine
    ${CMAKE_CURRENT_BINARY_DIR}/common_engine
)

# 2. Build Linux Native V4L2 Capture & v4l2loopback Sink Static Library
add_library(smartcam_linux_platform STATIC
    v4l2_virtual_camera_sink.cpp
    v4l2_camera_capture.cpp
    linux_bridge_c_api.cpp
)

target_include_directories(smartcam_linux_platform PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/../../common/include
    ${CMAKE_CURRENT_SOURCE_DIR}
)

target_link_libraries(smartcam_linux_platform PUBLIC
    smartcam_engine_static
    Threads::Threads
    ${CMAKE_DL_LIBS}
)

# 3. Embed Linux V4L2 Capture & v4l2loopback Sink C ABI directly into libsmartcam_engine.so
if(TARGET smartcam_engine)
    target_sources(smartcam_engine PRIVATE
        v4l2_virtual_camera_sink.cpp
        v4l2_camera_capture.cpp
        linux_bridge_c_api.cpp
    )
    target_include_directories(smartcam_engine PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}
    )
    target_link_libraries(smartcam_engine PRIVATE
        Threads::Threads
        ${CMAKE_DL_LIBS}
    )
endif()

# 4. Build standalone libsmartcam_linux_bridge.so shared library
add_library(smartcam_linux_bridge SHARED
    v4l2_virtual_camera_sink.cpp
    v4l2_camera_capture.cpp
    linux_bridge_c_api.cpp
)

target_include_directories(smartcam_linux_bridge PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/../../common/include
    ${CMAKE_CURRENT_SOURCE_DIR}
)

target_link_libraries(smartcam_linux_bridge PRIVATE
    smartcam_engine_static
    Threads::Threads
    ${CMAKE_DL_LIBS}
)

# 5. Build Native C++17 Linux Executable (`SmartCam`)
add_executable(SmartCam
    smartcam_linux_main.cpp
)

target_include_directories(SmartCam PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/../../common/include
    ${CMAKE_CURRENT_SOURCE_DIR}/../../common/engine
    ${CMAKE_CURRENT_SOURCE_DIR}
)

target_link_libraries(SmartCam PRIVATE
    smartcam_linux_platform
    smartcam_engine_static
    Threads::Threads
    ${CMAKE_DL_LIBS}
)

# 6. Apply SIMD flags (-mavx2 / -mavx512*) and Linux runtime link flags (-ldl -lpthread)
if(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    foreach(lnx_tgt IN ITEMS smartcam_linux_platform smartcam_linux_bridge SmartCam)
        if(SMARTCAM_ENABLE_AVX512)
            target_compile_options(${lnx_tgt} PRIVATE
                -O3 -mavx2 -mfma -mavx512f -mavx512bw -mavx512dq -mavx512vl -Wall -Wextra -Wno-format-truncation
            )
        else()
            target_compile_options(${lnx_tgt} PRIVATE
                -O3 -mavx2 -mfma -Wall -Wextra -Wno-format-truncation
            )
        endif()
    endforeach()
endif()

if(TARGET test_engine_cpp)
    target_link_libraries(test_engine_cpp PRIVATE
        Threads::Threads
        ${CMAKE_DL_LIBS}
    )
endif()

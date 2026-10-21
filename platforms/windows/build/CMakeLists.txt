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
project(SmartCamWindows VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT WIN32)
    message(FATAL_ERROR "platforms/windows/build/CMakeLists.txt must only be built on Windows.")
endif()

# Place all built DLLs, LIBs, and EXEs directly in the variant build directory
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
set(CMAKE_LIBRARY_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")
foreach(cfg IN ITEMS RELEASE DEBUG RELWITHDEBINFO MINSIZEREL)
    set(CMAKE_RUNTIME_OUTPUT_DIRECTORY_${cfg} "${CMAKE_BINARY_DIR}")
    set(CMAKE_LIBRARY_OUTPUT_DIRECTORY_${cfg} "${CMAKE_BINARY_DIR}")
    set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY_${cfg} "${CMAKE_BINARY_DIR}")
endforeach()

enable_testing()

# 1. Include platform-agnostic common engine (CPU AVX-512 / AVX2 / Scalar + GPU Vulkan / OpenCL / CUDA / ROCm)
add_subdirectory(
    ${CMAKE_CURRENT_SOURCE_DIR}/../../../common/engine
    ${CMAKE_CURRENT_BINARY_DIR}/common_engine
)

# 2. Build Windows 11 Media Foundation + DirectShow COM Virtual Camera Source DLL (smartcam_mf_source.dll)
add_library(smartcam_mf_source SHARED
    ../drivers/media_foundation/mf_media_source.cpp
    ../drivers/directshow/dshow_virtual_camera_source.cpp
    ../drivers/media_foundation/dllmain.cpp
    ../drivers/media_foundation/smartcam_mf_source.def
)

set_target_properties(smartcam_mf_source PROPERTIES PREFIX "")

target_include_directories(smartcam_mf_source PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/../../../common/include
    ${CMAKE_CURRENT_SOURCE_DIR}/..
)

target_link_libraries(smartcam_mf_source PRIVATE
    mfplat
    mfuuid
    strmiids
    uuid
    propsys
    ole32
    oleaut32
    advapi32
)

# 3. Build Windows Native Capture & Virtual Camera Sink Library (smartcam_win_platform)
add_library(smartcam_win_platform STATIC
    ../drivers/media_foundation/mf_virtual_camera_sink.cpp
    ../capture/mf_camera_capture.cpp
    ../win_bridge_c_api.cpp
)

target_include_directories(smartcam_win_platform PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/../../../common/include
    ${CMAKE_CURRENT_SOURCE_DIR}/..
)

target_link_libraries(smartcam_win_platform PUBLIC
    smartcam_engine_static
    mf
    mfplat
    mfreadwrite
    mfuuid
    ole32
)

# 4. Also embed Windows Native Bridge C ABI directly into smartcam_engine.dll AND smartcam_win_bridge.dll
if(TARGET smartcam_engine)
    target_sources(smartcam_engine PRIVATE
        ../drivers/media_foundation/mf_virtual_camera_sink.cpp
        ../capture/mf_camera_capture.cpp
        ../win_bridge_c_api.cpp
    )
    target_include_directories(smartcam_engine PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/..
    )
    target_link_libraries(smartcam_engine PRIVATE
        mf
        mfplat
        mfreadwrite
        mfuuid
        ole32
    )
endif()

add_library(smartcam_win_bridge SHARED
    ../drivers/media_foundation/mf_virtual_camera_sink.cpp
    ../capture/mf_camera_capture.cpp
    ../win_bridge_c_api.cpp
)

set_target_properties(smartcam_win_bridge PROPERTIES PREFIX "")

target_include_directories(smartcam_win_bridge PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/../../../common/include
    ${CMAKE_CURRENT_SOURCE_DIR}/..
)

target_compile_definitions(smartcam_win_bridge PRIVATE SMARTCAM_WIN_BRIDGE_EXPORTS=1)

target_link_libraries(smartcam_win_bridge PRIVATE
    smartcam_engine_static
    mf
    mfplat
    mfreadwrite
    mfuuid
    ole32
)

if(MSVC)
    target_compile_options(smartcam_mf_source PRIVATE /O2 /W4 /EHsc)
    foreach(win_tgt IN ITEMS smartcam_win_platform smartcam_win_bridge)
        if(SMARTCAM_ENABLE_AVX512)
            target_compile_options(${win_tgt} PRIVATE /O2 /arch:AVX512 /W4 /EHsc)
        else()
            target_compile_options(${win_tgt} PRIVATE /O2 /arch:AVX2 /W4 /EHsc)
        endif()
    endforeach()
elseif(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
    target_compile_options(smartcam_mf_source PRIVATE -O3 -Wall -Wextra -Wno-unused-parameter)
    foreach(win_tgt IN ITEMS smartcam_win_platform smartcam_win_bridge)
        target_compile_options(${win_tgt} PRIVATE -O3 -mavx2 -mfma -Wall -Wextra)
    endforeach()
    if(MINGW)
        target_link_options(smartcam_mf_source PRIVATE
            -static-libgcc
            -static-libstdc++
            -Wl,-Bstatic,--whole-archive -lwinpthread -Wl,--no-whole-archive,-Bdynamic
            -Wl,--kill-at
        )
        target_link_options(smartcam_win_bridge PRIVATE
            -static-libgcc
            -static-libstdc++
            -Wl,-Bstatic,--whole-archive -lwinpthread -Wl,--no-whole-archive,-Bdynamic
        )
    endif()
endif()

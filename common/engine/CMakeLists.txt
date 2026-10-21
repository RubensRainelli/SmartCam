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
project(smartcam_common_engine VERSION 1.0.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

option(SMARTCAM_BUILD_SHARED_ENGINE "Build smartcam_engine as a shared library (DLL/.so/.dylib) for Python ctypes" ON)
option(SMARTCAM_ENABLE_AVX512 "Enable AVX-512 compile flags alongside runtime CPUID dispatch" OFF)
option(SMARTCAM_ENABLE_TESTS "Build platform-agnostic C++ unit tests for common/engine" ON)

set(SMARTCAM_ENGINE_SOURCES
    crop_scaler/crop_scaler.cpp
    compositor/compositor.cpp
    gpu/gpu_accelerator.cpp
    segmentation/green_screen_segmenter.cpp
    segmentation/ai_onnx_segmenter.cpp
    ../plugins/phone_camera_plugin.cpp
    ../plugins/webrtc_rtsp_stub.cpp
    c_api.cpp
)

add_library(smartcam_engine_static STATIC ${SMARTCAM_ENGINE_SOURCES})
target_include_directories(smartcam_engine_static PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/../include
    ${CMAKE_CURRENT_SOURCE_DIR}
)
target_compile_definitions(smartcam_engine_static PUBLIC SMARTCAM_ENGINE_STATIC=1)

if(SMARTCAM_BUILD_SHARED_ENGINE)
    add_library(smartcam_engine SHARED ${SMARTCAM_ENGINE_SOURCES})
    target_include_directories(smartcam_engine PUBLIC
        ${CMAKE_CURRENT_SOURCE_DIR}/../include
        ${CMAKE_CURRENT_SOURCE_DIR}
    )
    target_compile_definitions(smartcam_engine PRIVATE SMARTCAM_ENGINE_EXPORTS=1)
    if(WIN32)
        set_target_properties(smartcam_engine PROPERTIES PREFIX "")
    endif()
    if(MINGW)
        target_link_options(smartcam_engine PRIVATE
            -static-libgcc
            -static-libstdc++
            -Wl,-Bstatic,--whole-archive -lwinpthread -Wl,--no-whole-archive,-Bdynamic
        )
    endif()

    if(WIN32)
        if(SMARTCAM_ENABLE_AVX512)
            set(SMARTCAM_VARIANT_DLL_NAME "smartcam_engine_avx512.dll")
        else()
            set(SMARTCAM_VARIANT_DLL_NAME "smartcam_engine_avx2.dll")
        endif()
        add_custom_command(TARGET smartcam_engine POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "$<TARGET_FILE:smartcam_engine>"
                "$<TARGET_FILE_DIR:smartcam_engine>/${SMARTCAM_VARIANT_DLL_NAME}"
            COMMENT "Creating variant-tagged engine DLL: ${SMARTCAM_VARIANT_DLL_NAME}"
        )
    else()
        if(SMARTCAM_ENABLE_AVX512)
            set(SMARTCAM_VARIANT_SO_NAME "libsmartcam_engine_avx512.so")
        else()
            set(SMARTCAM_VARIANT_SO_NAME "libsmartcam_engine_avx2.so")
        endif()
        add_custom_command(TARGET smartcam_engine POST_BUILD
            COMMAND ${CMAKE_COMMAND} -E copy_if_different
                "$<TARGET_FILE:smartcam_engine>"
                "$<TARGET_FILE_DIR:smartcam_engine>/${SMARTCAM_VARIANT_SO_NAME}"
            COMMENT "Creating variant-tagged engine shared library: ${SMARTCAM_VARIANT_SO_NAME}"
        )
    endif()
endif()

# Configure SIMD vectorization flags (AVX2 by default, optional AVX-512, with safe runtime CPUID dispatch)
foreach(tgt IN ITEMS smartcam_engine_static smartcam_engine)
    if(TARGET ${tgt})
        if(SMARTCAM_ENABLE_AVX512)
            target_compile_definitions(${tgt} PUBLIC SMARTCAM_ENABLE_AVX512_BUILD=1)
        else()
            target_compile_definitions(${tgt} PUBLIC SMARTCAM_ENABLE_AVX512_BUILD=0)
        endif()

        if(MSVC)
            if(SMARTCAM_ENABLE_AVX512)
                target_compile_options(${tgt} PRIVATE /O2 /arch:AVX512 /W4 /EHsc)
            else()
                target_compile_options(${tgt} PRIVATE /O2 /arch:AVX2 /W4 /EHsc)
            endif()
        elseif(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
            if(SMARTCAM_ENABLE_AVX512)
                target_compile_options(${tgt} PRIVATE -O3 -mavx2 -mfma -Wall -Wextra)
                set_source_files_properties(compositor/compositor.cpp PROPERTIES
                    COMPILE_OPTIONS "-mavx512f;-mavx512bw;-mavx512dq;-mavx512vl"
                )
            else()
                target_compile_options(${tgt} PRIVATE -O3 -mavx2 -mfma -Wall -Wextra)
            endif()
        endif()
    endif()
endforeach()

if(SMARTCAM_ENABLE_TESTS)
    enable_testing()
    add_executable(test_engine_cpp ../tests/test_engine_cpp.cpp)
    target_link_libraries(test_engine_cpp PRIVATE smartcam_engine_static)
    if(MSVC)
        if(SMARTCAM_ENABLE_AVX512)
            target_compile_options(test_engine_cpp PRIVATE /O2 /arch:AVX512 /W4 /EHsc)
        else()
            target_compile_options(test_engine_cpp PRIVATE /O2 /arch:AVX2 /W4 /EHsc)
        endif()
    elseif(CMAKE_CXX_COMPILER_ID MATCHES "GNU|Clang")
        target_compile_options(test_engine_cpp PRIVATE -O3 -mavx2 -mfma -Wall -Wextra)
        if(MINGW)
            target_link_options(test_engine_cpp PRIVATE
                -static-libgcc
                -static-libstdc++
                -Wl,-Bstatic,--whole-archive -lwinpthread -Wl,--no-whole-archive,-Bdynamic
            )
        endif()
    endif()
    add_test(NAME SmartCamCommonEngineTests COMMAND test_engine_cpp)
endif()

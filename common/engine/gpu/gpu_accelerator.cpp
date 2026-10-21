/*
 * Personal and Educational Free License (PEF License / PEFL)
 * Version 1.0 - 2026-05-26
 * Copyright (c) 2026 Rubens Rainelli
 *
 * Contact Details:
 *   Email: rubens@rainelli.it
 *   GitHub: https://github.com/RubensRainelli/
 *   Tel: (+39) 342 811 5882
 *
 * Personal and educational free use permitted.
 * Corporate, business or enterprise use prohibited without written authorization and fee.
 * The full license can be found in the LICENSE file.
 */

#include "gpu_accelerator.h"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>
#include <vector>

#include "../segmentation/ai_onnx_segmenter.h"

#if defined(_WIN32)
    #ifndef WIN32_LEAN_AND_MEAN
        #define WIN32_LEAN_AND_MEAN
    #endif
    #ifndef NOMINMAX
        #define NOMINMAX
    #endif
    #include <windows.h>
#else
    #include <dlfcn.h>
    #include <unistd.h>
#endif

#if defined(__x86_64__) || defined(_M_X64) || defined(__i386__) || defined(_M_IX86)
    #define SMARTCAM_GPU_X86 1
    #include <immintrin.h>
    #if defined(_MSC_VER)
        #include <intrin.h>
    #elif defined(__GNUC__) || defined(__clang__)
        #include <cpuid.h>
    #endif
#else
    #define SMARTCAM_GPU_X86 0
#endif

#if defined(_WIN32) && !defined(_WIN64)
    #define SMARTCAM_DRV_CALL __stdcall
#else
    #define SMARTCAM_DRV_CALL
#endif

namespace smartcam {
namespace common {
namespace engine {

namespace {

using DynLibHandle = void*;

DynLibHandle OpenDynamicLibrary(const char* const* candidate_names, size_t count) noexcept {
    for (size_t i = 0; i < count; ++i) {
        if (!candidate_names[i]) continue;
#if defined(_WIN32)
        HMODULE h = ::LoadLibraryA(candidate_names[i]);
        if (h != nullptr) {
            return reinterpret_cast<DynLibHandle>(h);
        }
#else
        void* h = ::dlopen(candidate_names[i], RTLD_NOW | RTLD_LOCAL);
        if (h != nullptr) {
            return h;
        }
#endif
    }
    return nullptr;
}

void CloseDynamicLibrary(DynLibHandle handle) noexcept {
    if (!handle) return;
#if defined(_WIN32)
    ::FreeLibrary(reinterpret_cast<HMODULE>(handle));
#else
    ::dlclose(handle);
#endif
}

template <typename FnPtr>
bool LoadSymbol(DynLibHandle handle, const char* symbol_name, FnPtr& out_fn) noexcept {
    out_fn = nullptr;
    if (!handle || !symbol_name) return false;
#if defined(_WIN32)
    void* raw = reinterpret_cast<void*>(::GetProcAddress(reinterpret_cast<HMODULE>(handle), symbol_name));
#else
    void* raw = ::dlsym(handle, symbol_name);
#endif
    if (!raw) return false;
    std::memcpy(&out_fn, &raw, sizeof(FnPtr));
    return out_fn != nullptr;
}

void CopyFixedString(char* dst, size_t dst_cap, const char* src) noexcept {
    if (!dst || dst_cap == 0) return;
    if (!src) {
        dst[0] = '\0';
        return;
    }
    const size_t len = std::strlen(src);
    const size_t copy_len = (len < dst_cap - 1) ? len : (dst_cap - 1);
    std::memcpy(dst, src, copy_len);
    dst[copy_len] = '\0';
}

void TrimAsciiWhitespace(char* str) noexcept {
    if (!str) return;
    size_t len = std::strlen(str);
    while (len > 0 && (str[len - 1] == ' ' || str[len - 1] == '\t' || str[len - 1] == '\r' || str[len - 1] == '\n')) {
        str[--len] = '\0';
    }
    size_t start = 0;
    while (str[start] == ' ' || str[start] == '\t') {
        ++start;
    }
    if (start > 0) {
        std::memmove(str, str + start, len - start + 1);
    }
}

bool ContainsCaseInsensitive(const char* haystack, const char* needle) noexcept {
    if (!haystack || !needle) return false;
    const size_t h_len = std::strlen(haystack);
    const size_t n_len = std::strlen(needle);
    if (n_len == 0 || n_len > h_len) return false;
    for (size_t i = 0; i + n_len <= h_len; ++i) {
        bool match = true;
        for (size_t j = 0; j < n_len; ++j) {
            char c1 = haystack[i + j];
            char c2 = needle[j];
            if (c1 >= 'a' && c1 <= 'z') c1 = static_cast<char>(c1 - 'a' + 'A');
            if (c2 >= 'a' && c2 <= 'z') c2 = static_cast<char>(c2 - 'a' + 'A');
            if (c1 != c2) {
                match = false;
                break;
            }
        }
        if (match) return true;
    }
    return false;
}

bool InferIntegratedGpuFromName(const char* device_name, const char* vendor_name) noexcept {
    if (ContainsCaseInsensitive(device_name, "Iris") ||
        ContainsCaseInsensitive(device_name, "UHD Graphics") ||
        ContainsCaseInsensitive(device_name, "HD Graphics") ||
        ContainsCaseInsensitive(device_name, "Radeon(TM) Graphics") ||
        ContainsCaseInsensitive(device_name, "Radeon Graphics") ||
        ContainsCaseInsensitive(device_name, "Vega") ||
        ContainsCaseInsensitive(device_name, "Radeon 680M") ||
        ContainsCaseInsensitive(device_name, "Radeon 780M") ||
        ContainsCaseInsensitive(device_name, "Adreno") ||
        ContainsCaseInsensitive(device_name, "Apple M")) {
        return true;
    }
    if (ContainsCaseInsensitive(vendor_name, "Intel") &&
        !ContainsCaseInsensitive(device_name, "Arc")) {
        return true;
    }
    return false;
}

void QueryCpuBrandString(char* out_name, size_t cap) noexcept {
    CopyFixedString(out_name, cap, "Generic x86_64 / ARM64 Processor");
#if SMARTCAM_GPU_X86
    int regs[4] = {0, 0, 0, 0};
#if defined(_MSC_VER)
    __cpuid(regs, static_cast<int>(0x80000000u));
#elif defined(__GNUC__) || defined(__clang__)
    __cpuid(0x80000000u, regs[0], regs[1], regs[2], regs[3]);
#endif
    const uint32_t max_ext = static_cast<uint32_t>(regs[0]);
    if (max_ext >= 0x80000004u) {
        char brand[64] = {0};
        for (uint32_t leaf = 0x80000002u; leaf <= 0x80000004u; ++leaf) {
#if defined(_MSC_VER)
            __cpuid(regs, static_cast<int>(leaf));
#elif defined(__GNUC__) || defined(__clang__)
            __cpuid(leaf, regs[0], regs[1], regs[2], regs[3]);
#endif
            std::memcpy(brand + (leaf - 0x80000002u) * 16, regs, 16);
        }
        brand[48] = '\0';
        TrimAsciiWhitespace(brand);
        if (brand[0] != '\0') {
            CopyFixedString(out_name, cap, brand);
        }
    }
#endif
}

uint64_t QuerySystemRamBytes() noexcept {
#if defined(_WIN32)
    MEMORYSTATUSEX mem_stat{};
    mem_stat.dwLength = sizeof(mem_stat);
    if (::GlobalMemoryStatusEx(&mem_stat)) {
        return static_cast<uint64_t>(mem_stat.ullTotalPhys);
    }
    return 8ULL * 1024ULL * 1024ULL * 1024ULL;
#else
    const long pages = ::sysconf(_SC_PHYS_PAGES);
    const long page_size = ::sysconf(_SC_PAGESIZE);
    if (pages > 0 && page_size > 0) {
        return static_cast<uint64_t>(pages) * static_cast<uint64_t>(page_size);
    }
    return 8ULL * 1024ULL * 1024ULL * 1024ULL;
#endif
}

// ============================================================================
// Minimal ABI Definitions for Vulkan 1.0 Dynamic Loader (Zero SDK Dependency)
// ============================================================================
using VkResult           = int32_t;
using VkFlags            = uint32_t;
using VkBool32           = uint32_t;
using VkDeviceSize       = uint64_t;
using VkInstance         = struct VkInstance_T*;
using VkPhysicalDevice   = struct VkPhysicalDevice_T*;
using VkDevice           = struct VkDevice_T*;
using VkQueue            = struct VkQueue_T*;
using VkCommandPool      = uint64_t;
using VkBuffer           = uint64_t;
using VkDeviceMemory     = uint64_t;
using VkCommandBuffer    = struct VkCommandBuffer_T*;
using PFN_vkVoidFunction = void (SMARTCAM_DRV_CALL*)();

constexpr VkResult VK_SUCCESS_VAL                              = 0;
constexpr uint32_t VK_STRUCTURE_TYPE_APPLICATION_INFO_VAL      = 0;
constexpr uint32_t VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO_VAL  = 1;
constexpr uint32_t VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO_VAL = 2;
constexpr uint32_t VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO_VAL    = 3;
constexpr uint32_t VK_STRUCTURE_TYPE_SUBMIT_INFO_VAL           = 4;
constexpr uint32_t VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO_VAL  = 5;
constexpr uint32_t VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO_VAL    = 12;
constexpr uint32_t VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO_VAL = 39;
constexpr uint32_t VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO_VAL = 40;
constexpr uint32_t VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO_VAL = 42;

constexpr uint32_t VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU_VAL = 1;
constexpr uint32_t VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU_VAL   = 2;
constexpr uint32_t VK_QUEUE_COMPUTE_BIT_VAL                   = 0x00000002u;
constexpr uint32_t VK_MEMORY_HEAP_DEVICE_LOCAL_BIT_VAL        = 0x00000001u;
constexpr uint32_t VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT_VAL    = 0x00000002u;
constexpr uint32_t VK_MEMORY_PROPERTY_HOST_COHERENT_BIT_VAL   = 0x00000004u;
constexpr uint32_t VK_BUFFER_USAGE_TRANSFER_SRC_BIT_VAL       = 0x00000001u;
constexpr uint32_t VK_BUFFER_USAGE_TRANSFER_DST_BIT_VAL       = 0x00000002u;
constexpr uint32_t VK_BUFFER_USAGE_STORAGE_BUFFER_BIT_VAL     = 0x00000020u;
constexpr uint32_t VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT_VAL = 0x00000002u;
constexpr uint32_t VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT_VAL     = 0x00000001u;

struct VkApplicationInfo_Min {
    uint32_t    sType;
    const void* pNext;
    const char* pApplicationName;
    uint32_t    applicationVersion;
    const char* pEngineName;
    uint32_t    engineVersion;
    uint32_t    apiVersion;
};

struct VkInstanceCreateInfo_Min {
    uint32_t                     sType;
    const void*                  pNext;
    VkFlags                      flags;
    const VkApplicationInfo_Min* pApplicationInfo;
    uint32_t                     enabledLayerCount;
    const char* const*           ppEnabledLayerNames;
    uint32_t                     enabledExtensionCount;
    const char* const*           ppEnabledExtensionNames;
};

struct VkPhysicalDeviceLimits_Pad {
    uint8_t raw_bytes[504];
};

struct VkPhysicalDeviceSparseProperties_Pad {
    VkBool32 residencyStandard2DBlockShape;
    VkBool32 residencyStandard2DMultisampleBlockShape;
    VkBool32 residencyStandard3DBlockShape;
    VkBool32 residencyAlignedMipSize;
    VkBool32 residencyNonResidentStrict;
};

struct VkPhysicalDeviceProperties_Min {
    uint32_t                                 apiVersion;
    uint32_t                                 driverVersion;
    uint32_t                                 vendorID;
    uint32_t                                 deviceID;
    uint32_t                                 deviceType;
    char                                     deviceName[256];
    uint8_t                                  pipelineCacheUUID[16];
    VkPhysicalDeviceLimits_Pad               limits;
    VkPhysicalDeviceSparseProperties_Pad     sparseProperties;
};

struct VkMemoryType_Min {
    VkFlags  propertyFlags;
    uint32_t heapIndex;
};

struct VkMemoryHeap_Min {
    VkDeviceSize size;
    VkFlags      flags;
};

struct VkPhysicalDeviceMemoryProperties_Min {
    uint32_t         memoryTypeCount;
    VkMemoryType_Min memoryTypes[32];
    uint32_t         memoryHeapCount;
    VkMemoryHeap_Min memoryHeaps[16];
};

struct VkExtent3D_Min {
    uint32_t width;
    uint32_t height;
    uint32_t depth;
};

struct VkQueueFamilyProperties_Min {
    VkFlags        queueFlags;
    uint32_t       queueCount;
    uint32_t       timestampValidBits;
    VkExtent3D_Min minImageTransferGranularity;
};

struct VkDeviceQueueCreateInfo_Min {
    uint32_t    sType;
    const void* pNext;
    VkFlags     flags;
    uint32_t    queueFamilyIndex;
    uint32_t    queueCount;
    const float* pQueuePriorities;
};

struct VkDeviceCreateInfo_Min {
    uint32_t                           sType;
    const void*                        pNext;
    VkFlags                            flags;
    uint32_t                           queueCreateInfoCount;
    const VkDeviceQueueCreateInfo_Min* pQueueCreateInfos;
    uint32_t                           enabledLayerCount;
    const char* const*                 ppEnabledLayerNames;
    uint32_t                           enabledExtensionCount;
    const char* const*                 ppEnabledExtensionNames;
    const void*                        pEnabledFeatures;
};

struct VkBufferCreateInfo_Min {
    uint32_t     sType;
    const void*  pNext;
    VkFlags      flags;
    VkDeviceSize size;
    VkFlags      usage;
    uint32_t     sharingMode;
    uint32_t     queueFamilyIndexCount;
    const uint32_t* pQueueFamilyIndices;
};

struct VkMemoryRequirements_Min {
    VkDeviceSize size;
    VkDeviceSize alignment;
    uint32_t     memoryTypeBits;
};

struct VkMemoryAllocateInfo_Min {
    uint32_t     sType;
    const void*  pNext;
    VkDeviceSize allocationSize;
    uint32_t     memoryTypeIndex;
};

struct VkCommandPoolCreateInfo_Min {
    uint32_t    sType;
    const void* pNext;
    VkFlags     flags;
    uint32_t    queueFamilyIndex;
};

struct VkCommandBufferAllocateInfo_Min {
    uint32_t      sType;
    const void*   pNext;
    VkCommandPool commandPool;
    uint32_t      level;
    uint32_t      commandBufferCount;
};

struct VkCommandBufferBeginInfo_Min {
    uint32_t    sType;
    const void* pNext;
    VkFlags     flags;
    const void* pInheritanceInfo;
};

struct VkBufferCopy_Min {
    VkDeviceSize srcOffset;
    VkDeviceSize dstOffset;
    VkDeviceSize size;
};

struct VkSubmitInfo_Min {
    uint32_t               sType;
    const void*            pNext;
    uint32_t               waitSemaphoreCount;
    const void*            pWaitSemaphores;
    const VkFlags*         pWaitDstStageMask;
    uint32_t               commandBufferCount;
    const VkCommandBuffer* pCommandBuffers;
    uint32_t               signalSemaphoreCount;
    const void*            pSignalSemaphores;
};

using PFN_vkGetInstanceProcAddr = PFN_vkVoidFunction(SMARTCAM_DRV_CALL*)(VkInstance, const char*);
using PFN_vkCreateInstance      = VkResult(SMARTCAM_DRV_CALL*)(const VkInstanceCreateInfo_Min*, const void*, VkInstance*);
using PFN_vkDestroyInstance     = void(SMARTCAM_DRV_CALL*)(VkInstance, const void*);
using PFN_vkEnumeratePhysicalDevices = VkResult(SMARTCAM_DRV_CALL*)(VkInstance, uint32_t*, VkPhysicalDevice*);
using PFN_vkGetPhysicalDeviceProperties = void(SMARTCAM_DRV_CALL*)(VkPhysicalDevice, VkPhysicalDeviceProperties_Min*);
using PFN_vkGetPhysicalDeviceMemoryProperties = void(SMARTCAM_DRV_CALL*)(VkPhysicalDevice, VkPhysicalDeviceMemoryProperties_Min*);
using PFN_vkGetPhysicalDeviceQueueFamilyProperties = void(SMARTCAM_DRV_CALL*)(VkPhysicalDevice, uint32_t*, VkQueueFamilyProperties_Min*);
using PFN_vkCreateDevice        = VkResult(SMARTCAM_DRV_CALL*)(VkPhysicalDevice, const VkDeviceCreateInfo_Min*, const void*, VkDevice*);
using PFN_vkDestroyDevice       = void(SMARTCAM_DRV_CALL*)(VkDevice, const void*);
using PFN_vkGetDeviceQueue      = void(SMARTCAM_DRV_CALL*)(VkDevice, uint32_t, uint32_t, VkQueue*);
using PFN_vkCreateBuffer        = VkResult(SMARTCAM_DRV_CALL*)(VkDevice, const VkBufferCreateInfo_Min*, const void*, VkBuffer*);
using PFN_vkDestroyBuffer       = void(SMARTCAM_DRV_CALL*)(VkDevice, VkBuffer, const void*);
using PFN_vkGetBufferMemoryRequirements = void(SMARTCAM_DRV_CALL*)(VkDevice, VkBuffer, VkMemoryRequirements_Min*);
using PFN_vkAllocateMemory      = VkResult(SMARTCAM_DRV_CALL*)(VkDevice, const VkMemoryAllocateInfo_Min*, const void*, VkDeviceMemory*);
using PFN_vkFreeMemory          = void(SMARTCAM_DRV_CALL*)(VkDevice, VkDeviceMemory, const void*);
using PFN_vkBindBufferMemory    = VkResult(SMARTCAM_DRV_CALL*)(VkDevice, VkBuffer, VkDeviceMemory, VkDeviceSize);
using PFN_vkMapMemory           = VkResult(SMARTCAM_DRV_CALL*)(VkDevice, VkDeviceMemory, VkDeviceSize, VkDeviceSize, VkFlags, void**);
using PFN_vkUnmapMemory         = void(SMARTCAM_DRV_CALL*)(VkDevice, VkDeviceMemory);
using PFN_vkCreateCommandPool   = VkResult(SMARTCAM_DRV_CALL*)(VkDevice, const VkCommandPoolCreateInfo_Min*, const void*, VkCommandPool*);
using PFN_vkDestroyCommandPool  = void(SMARTCAM_DRV_CALL*)(VkDevice, VkCommandPool, const void*);
using PFN_vkAllocateCommandBuffers = VkResult(SMARTCAM_DRV_CALL*)(VkDevice, const VkCommandBufferAllocateInfo_Min*, VkCommandBuffer*);
using PFN_vkBeginCommandBuffer  = VkResult(SMARTCAM_DRV_CALL*)(VkCommandBuffer, const VkCommandBufferBeginInfo_Min*);
using PFN_vkEndCommandBuffer    = VkResult(SMARTCAM_DRV_CALL*)(VkCommandBuffer);
using PFN_vkCmdCopyBuffer       = void(SMARTCAM_DRV_CALL*)(VkCommandBuffer, VkBuffer, VkBuffer, uint32_t, const VkBufferCopy_Min*);
using PFN_vkQueueSubmit         = VkResult(SMARTCAM_DRV_CALL*)(VkQueue, uint32_t, const VkSubmitInfo_Min*, uint64_t);
using PFN_vkQueueWaitIdle       = VkResult(SMARTCAM_DRV_CALL*)(VkQueue);

// ============================================================================
// Minimal ABI Definitions for OpenCL 1.2/2.0 Dynamic Loader (Zero SDK Dependency)
// ============================================================================
using cl_int               = int32_t;
using cl_uint              = uint32_t;
using cl_ulong             = uint64_t;
using cl_bool              = cl_uint;
using cl_bitfield          = cl_ulong;
using cl_device_type       = cl_bitfield;
using cl_platform_info     = cl_uint;
using cl_device_info       = cl_uint;
using cl_mem_flags         = cl_bitfield;
using cl_command_queue_properties = cl_bitfield;
using cl_program_build_info = cl_uint;
using cl_platform_id       = struct _cl_platform_id*;
using cl_device_id         = struct _cl_device_id*;
using cl_context           = struct _cl_context*;
using cl_command_queue     = struct _cl_command_queue*;
using cl_mem               = struct _cl_mem*;
using cl_program           = struct _cl_program*;
using cl_kernel            = struct _cl_kernel*;
using cl_event             = struct _cl_event*;

constexpr cl_int         CL_SUCCESS_VAL                   = 0;
constexpr cl_device_type CL_DEVICE_TYPE_GPU_VAL           = (1ULL << 2);
constexpr cl_device_type CL_DEVICE_TYPE_ALL_VAL           = 0xFFFFFFFFULL;
constexpr cl_device_info CL_DEVICE_MAX_COMPUTE_UNITS_VAL  = 0x1002;
constexpr cl_device_info CL_DEVICE_GLOBAL_MEM_SIZE_VAL    = 0x101F;
constexpr cl_device_info CL_DEVICE_NAME_VAL               = 0x102B;
constexpr cl_device_info CL_DEVICE_VENDOR_VAL             = 0x102C;
constexpr cl_device_info CL_DRIVER_VERSION_VAL            = 0x102D;
constexpr cl_device_info CL_DEVICE_VERSION_VAL            = 0x102F;
constexpr cl_device_info CL_DEVICE_HOST_UNIFIED_MEMORY_VAL = 0x1035;

constexpr cl_mem_flags   CL_MEM_READ_WRITE_VAL            = (1ULL << 0);
constexpr cl_mem_flags   CL_MEM_WRITE_ONLY_VAL            = (1ULL << 1);
constexpr cl_mem_flags   CL_MEM_READ_ONLY_VAL             = (1ULL << 2);
constexpr cl_mem_flags   CL_MEM_ALLOC_HOST_PTR_VAL        = (1ULL << 4);
constexpr cl_bool        CL_TRUE_VAL                      = 1;

using PFN_clGetPlatformIDs    = cl_int(SMARTCAM_DRV_CALL*)(cl_uint, cl_platform_id*, cl_uint*);
using PFN_clGetDeviceIDs      = cl_int(SMARTCAM_DRV_CALL*)(cl_platform_id, cl_device_type, cl_uint, cl_device_id*, cl_uint*);
using PFN_clGetDeviceInfo     = cl_int(SMARTCAM_DRV_CALL*)(cl_device_id, cl_device_info, size_t, void*, size_t*);
using PFN_clCreateContext     = cl_context(SMARTCAM_DRV_CALL*)(const intptr_t*, cl_uint, const cl_device_id*, void(SMARTCAM_DRV_CALL*)(const char*, const void*, size_t, void*), void*, cl_int*);
using PFN_clReleaseContext    = cl_int(SMARTCAM_DRV_CALL*)(cl_context);
using PFN_clCreateCommandQueue = cl_command_queue(SMARTCAM_DRV_CALL*)(cl_context, cl_device_id, cl_command_queue_properties, cl_int*);
using PFN_clCreateCommandQueueWithProperties = cl_command_queue(SMARTCAM_DRV_CALL*)(cl_context, cl_device_id, const intptr_t*, cl_int*);
using PFN_clReleaseCommandQueue = cl_int(SMARTCAM_DRV_CALL*)(cl_command_queue);
using PFN_clCreateProgramWithSource = cl_program(SMARTCAM_DRV_CALL*)(cl_context, cl_uint, const char**, const size_t*, cl_int*);
using PFN_clBuildProgram      = cl_int(SMARTCAM_DRV_CALL*)(cl_program, cl_uint, const cl_device_id*, const char*, void(SMARTCAM_DRV_CALL*)(cl_program, void*), void*);
using PFN_clReleaseProgram    = cl_int(SMARTCAM_DRV_CALL*)(cl_program);
using PFN_clCreateKernel      = cl_kernel(SMARTCAM_DRV_CALL*)(cl_program, const char*, cl_int*);
using PFN_clSetKernelArg      = cl_int(SMARTCAM_DRV_CALL*)(cl_kernel, cl_uint, size_t, const void*);
using PFN_clEnqueueNDRangeKernel = cl_int(SMARTCAM_DRV_CALL*)(cl_command_queue, cl_kernel, cl_uint, const size_t*, const size_t*, const size_t*, cl_uint, const cl_event*, cl_event*);
using PFN_clReleaseKernel     = cl_int(SMARTCAM_DRV_CALL*)(cl_kernel);
using PFN_clCreateBuffer      = cl_mem(SMARTCAM_DRV_CALL*)(cl_context, cl_mem_flags, size_t, void*, cl_int*);
using PFN_clEnqueueReadBuffer = cl_int(SMARTCAM_DRV_CALL*)(cl_command_queue, cl_mem, cl_bool, size_t, size_t, void*, cl_uint, const cl_event*, cl_event*);
using PFN_clEnqueueWriteBuffer = cl_int(SMARTCAM_DRV_CALL*)(cl_command_queue, cl_mem, cl_bool, size_t, size_t, const void*, cl_uint, const cl_event*, cl_event*);
using PFN_clReleaseMemObject  = cl_int(SMARTCAM_DRV_CALL*)(cl_mem);
using PFN_clFinish            = cl_int(SMARTCAM_DRV_CALL*)(cl_command_queue);

static const char* kOpenClKernelSource = R"CLC(
__kernel void smartcam_blend_bgra(
    __global const uchar4* fg,
    __global const uchar4* bg,
    __global const uchar*  mask,
    __global uchar4*       out_img,
    const uint             width,
    const uint             height,
    const uint             rgba_pitch_px,
    const uint             mask_stride)
{
    const uint x = get_global_id(0);
    const uint y = get_global_id(1);
    if (x >= width || y >= height) return;

    const uint px_idx   = y * rgba_pitch_px + x;
    const uint mask_idx = y * mask_stride + x;

    const uint4 f = convert_uint4(fg[px_idx]);
    const uint4 b = convert_uint4(bg[px_idx]);
    const uint  a_raw = (uint)mask[mask_idx];
    const uint  alpha = a_raw + (a_raw >> 7);
    const uint  inv_a = 256u - alpha;

    uint4 res;
    res.x = (f.x * alpha + b.x * inv_a) >> 8;
    res.y = (f.y * alpha + b.y * inv_a) >> 8;
    res.z = (f.z * alpha + b.z * inv_a) >> 8;
    res.w = 255u;

    out_img[px_idx] = convert_uchar4(res);
}

__kernel void smartcam_box_blur_h(
    __global const uchar4* src,
    __global uchar4*       dst,
    const uint             width,
    const uint             height,
    const uint             pitch_px,
    const int              radius)
{
    const int x = (int)get_global_id(0);
    const int y = (int)get_global_id(1);
    if (x >= (int)width || y >= (int)height) return;

    uint3 sum = (uint3)(0u, 0u, 0u);
    const int w_max = (int)width - 1;
    const int row_base = y * (int)pitch_px;

    for (int k = -radius; k <= radius; ++k) {
        const int sx = clamp(x + k, 0, w_max);
        const uchar4 p = src[row_base + sx];
        sum.x += (uint)p.x;
        sum.y += (uint)p.y;
        sum.z += (uint)p.z;
    }

    const uint diam = (uint)(2 * radius + 1);
    uchar4 out_px;
    out_px.x = (uchar)(sum.x / diam);
    out_px.y = (uchar)(sum.y / diam);
    out_px.z = (uchar)(sum.z / diam);
    out_px.w = (uchar)255;
    dst[row_base + x] = out_px;
}

__kernel void smartcam_box_blur_v(
    __global const uchar4* src,
    __global uchar4*       dst,
    const uint             width,
    const uint             height,
    const uint             pitch_px,
    const int              radius)
{
    const int x = (int)get_global_id(0);
    const int y = (int)get_global_id(1);
    if (x >= (int)width || y >= (int)height) return;

    uint3 sum = (uint3)(0u, 0u, 0u);
    const int h_max = (int)height - 1;

    for (int k = -radius; k <= radius; ++k) {
        const int sy = clamp(y + k, 0, h_max);
        const uchar4 p = src[sy * (int)pitch_px + x];
        sum.x += (uint)p.x;
        sum.y += (uint)p.y;
        sum.z += (uint)p.z;
    }

    const uint diam = (uint)(2 * radius + 1);
    uchar4 out_px;
    out_px.x = (uchar)(sum.x / diam);
    out_px.y = (uchar)(sum.y / diam);
    out_px.z = (uchar)(sum.z / diam);
    out_px.w = (uchar)255;
    dst[y * (int)pitch_px + x] = out_px;
}

__kernel void smartcam_ai_upsample(
    __global const float* prob_map,
    __global uchar*       out_mask,
    const uint            map_w,
    const uint            map_h,
    const uint            out_w,
    const uint            out_h,
    const uint            out_stride)
{
    const uint x = get_global_id(0);
    const uint y = get_global_id(1);
    if (x >= out_w || y >= out_h) return;

    const float gx = ((float)x + 0.5f) * ((float)map_w / (float)out_w) - 0.5f;
    const float gy = ((float)y + 0.5f) * ((float)map_h / (float)out_h) - 0.5f;

    const int x0 = clamp((int)floor(gx), 0, (int)map_w - 1);
    const int y0 = clamp((int)floor(gy), 0, (int)map_h - 1);
    const int x1 = clamp(x0 + 1, 0, (int)map_w - 1);
    const int y1 = clamp(y0 + 1, 0, (int)map_h - 1);

    const float fx = clamp(gx - floor(gx), 0.0f, 1.0f);
    const float fy = clamp(gy - floor(gy), 0.0f, 1.0f);

    const float p00 = prob_map[y0 * map_w + x0];
    const float p10 = prob_map[y0 * map_w + x1];
    const float p01 = prob_map[y1 * map_w + x0];
    const float p11 = prob_map[y1 * map_w + x1];

    const float top = mix(p00, p10, fx);
    const float bot = mix(p01, p11, fx);
    const float val = clamp(mix(top, bot, fy), 0.0f, 1.0f);

    out_mask[y * out_stride + x] = (uchar)(val * 255.0f + 0.5f);
}
)CLC";

// ============================================================================
// Minimal ABI Definitions for NVIDIA CUDA Driver API (Zero SDK Dependency)
// ============================================================================
using CUresult    = int32_t;
using CUdevice    = int32_t;
using CUcontext   = struct CUctx_st*;
using CUdeviceptr = uint64_t;

constexpr CUresult CUDA_SUCCESS_VAL                                 = 0;
constexpr int32_t  CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT_VAL     = 16;
constexpr int32_t  CU_DEVICE_ATTRIBUTE_INTEGRATED_VAL               = 18;
constexpr int32_t  CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR_VAL = 75;
constexpr int32_t  CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR_VAL = 76;

using PFN_cuInit              = CUresult(SMARTCAM_DRV_CALL*)(uint32_t);
using PFN_cuDriverGetVersion  = CUresult(SMARTCAM_DRV_CALL*)(int32_t*);
using PFN_cuDeviceGetCount    = CUresult(SMARTCAM_DRV_CALL*)(int32_t*);
using PFN_cuDeviceGet         = CUresult(SMARTCAM_DRV_CALL*)(CUdevice*, int32_t);
using PFN_cuDeviceGetName     = CUresult(SMARTCAM_DRV_CALL*)(char*, int32_t, CUdevice);
using PFN_cuDeviceTotalMem_v2 = CUresult(SMARTCAM_DRV_CALL*)(size_t*, CUdevice);
using PFN_cuDeviceGetAttribute = CUresult(SMARTCAM_DRV_CALL*)(int32_t*, int32_t, CUdevice);
using PFN_cuCtxCreate_v2      = CUresult(SMARTCAM_DRV_CALL*)(CUcontext*, uint32_t, CUdevice);
using PFN_cuCtxSetCurrent     = CUresult(SMARTCAM_DRV_CALL*)(CUcontext);
using PFN_cuCtxSynchronize    = CUresult(SMARTCAM_DRV_CALL*)();
using PFN_cuCtxDestroy_v2     = CUresult(SMARTCAM_DRV_CALL*)(CUcontext);
using PFN_cuMemAlloc_v2       = CUresult(SMARTCAM_DRV_CALL*)(CUdeviceptr*, size_t);
using PFN_cuMemFree_v2        = CUresult(SMARTCAM_DRV_CALL*)(CUdeviceptr);
using PFN_cuMemcpyHtoD_v2     = CUresult(SMARTCAM_DRV_CALL*)(CUdeviceptr, const void*, size_t);
using PFN_cuMemcpyDtoH_v2     = CUresult(SMARTCAM_DRV_CALL*)(void*, CUdeviceptr, size_t);

// ============================================================================
// Minimal ABI Definitions for AMD ROCm / HIP Runtime API (Zero SDK Dependency)
// ============================================================================
using hipError_t  = int32_t;
using hipDevice_t = int32_t;

constexpr hipError_t HIP_SUCCESS_VAL                      = 0;
constexpr int32_t    HIP_MEMCPY_HOST_TO_DEVICE_VAL        = 1;
constexpr int32_t    HIP_MEMCPY_DEVICE_TO_HOST_VAL        = 2;
constexpr int32_t    HIP_DEVICE_ATTR_MULTIPROCESSOR_COUNT = 65;
constexpr int32_t    HIP_DEVICE_ATTR_INTEGRATED           = 47;

using PFN_hipInit               = hipError_t(SMARTCAM_DRV_CALL*)(uint32_t);
using PFN_hipDriverGetVersion   = hipError_t(SMARTCAM_DRV_CALL*)(int32_t*);
using PFN_hipRuntimeGetVersion  = hipError_t(SMARTCAM_DRV_CALL*)(int32_t*);
using PFN_hipGetDeviceCount     = hipError_t(SMARTCAM_DRV_CALL*)(int32_t*);
using PFN_hipDeviceGet          = hipError_t(SMARTCAM_DRV_CALL*)(hipDevice_t*, int32_t);
using PFN_hipDeviceGetName      = hipError_t(SMARTCAM_DRV_CALL*)(char*, int32_t, hipDevice_t);
using PFN_hipDeviceTotalMem     = hipError_t(SMARTCAM_DRV_CALL*)(size_t*, hipDevice_t);
using PFN_hipMemGetInfo         = hipError_t(SMARTCAM_DRV_CALL*)(size_t*, size_t*);
using PFN_hipDeviceGetAttribute = hipError_t(SMARTCAM_DRV_CALL*)(int32_t*, int32_t, hipDevice_t);
using PFN_hipMalloc             = hipError_t(SMARTCAM_DRV_CALL*)(void**, size_t);
using PFN_hipFree               = hipError_t(SMARTCAM_DRV_CALL*)(void*);
using PFN_hipMemcpy             = hipError_t(SMARTCAM_DRV_CALL*)(void*, const void*, size_t, int32_t);
using PFN_hipDeviceSynchronize  = hipError_t(SMARTCAM_DRV_CALL*)();

}  // namespace

// ============================================================================
// GpuAccelerator::RuntimeImpl
// ============================================================================
struct GpuAccelerator::RuntimeImpl {
    mutable std::mutex              mutex;
    SimdCompositor                  cpu_compositor;
    ComputeBackend                  requested_backend = ComputeBackend::AUTO;
    ComputeBackend                  active_backend    = ComputeBackend::CPU;
    std::vector<ComputeDeviceInfo>  devices;
    std::vector<float>              ema_prob_cache;

    // --- Vulkan Dynamic State ---
    DynLibHandle                    vk_lib            = nullptr;
    VkInstance                      vk_instance       = nullptr;
    VkPhysicalDevice                vk_phys_dev       = nullptr;
    VkDevice                        vk_device         = nullptr;
    VkQueue                         vk_queue          = nullptr;
    VkCommandPool                   vk_cmd_pool       = 0;
    VkCommandBuffer                 vk_cmd_buf        = nullptr;
    VkBuffer                        vk_staging_src    = 0;
    VkDeviceMemory                  vk_staging_src_mem = 0;
    void*                           vk_mapped_src     = nullptr;
    VkBuffer                        vk_staging_dst    = 0;
    VkDeviceMemory                  vk_staging_dst_mem = 0;
    void*                           vk_mapped_dst     = nullptr;
    size_t                          vk_buffer_cap     = 0;
    uint32_t                        vk_compute_q_fam  = 0;
    uint32_t                        vk_host_mem_type  = UINT32_MAX;
    PFN_vkGetInstanceProcAddr       pfn_vkGetInstanceProcAddr = nullptr;
    PFN_vkCreateInstance            pfn_vkCreateInstance      = nullptr;
    PFN_vkDestroyInstance           pfn_vkDestroyInstance     = nullptr;
    PFN_vkEnumeratePhysicalDevices  pfn_vkEnumeratePhysicalDevices = nullptr;
    PFN_vkGetPhysicalDeviceProperties pfn_vkGetPhysicalDeviceProperties = nullptr;
    PFN_vkGetPhysicalDeviceMemoryProperties pfn_vkGetPhysicalDeviceMemoryProperties = nullptr;
    PFN_vkGetPhysicalDeviceQueueFamilyProperties pfn_vkGetPhysicalDeviceQueueFamilyProperties = nullptr;
    PFN_vkCreateDevice              pfn_vkCreateDevice        = nullptr;
    PFN_vkDestroyDevice             pfn_vkDestroyDevice       = nullptr;
    PFN_vkGetDeviceQueue            pfn_vkGetDeviceQueue      = nullptr;
    PFN_vkCreateBuffer              pfn_vkCreateBuffer        = nullptr;
    PFN_vkDestroyBuffer             pfn_vkDestroyBuffer       = nullptr;
    PFN_vkGetBufferMemoryRequirements pfn_vkGetBufferMemoryRequirements = nullptr;
    PFN_vkAllocateMemory            pfn_vkAllocateMemory      = nullptr;
    PFN_vkFreeMemory                pfn_vkFreeMemory          = nullptr;
    PFN_vkBindBufferMemory          pfn_vkBindBufferMemory    = nullptr;
    PFN_vkMapMemory                 pfn_vkMapMemory           = nullptr;
    PFN_vkUnmapMemory               pfn_vkUnmapMemory         = nullptr;
    PFN_vkCreateCommandPool         pfn_vkCreateCommandPool   = nullptr;
    PFN_vkDestroyCommandPool        pfn_vkDestroyCommandPool  = nullptr;
    PFN_vkAllocateCommandBuffers    pfn_vkAllocateCommandBuffers = nullptr;
    PFN_vkBeginCommandBuffer        pfn_vkBeginCommandBuffer  = nullptr;
    PFN_vkEndCommandBuffer          pfn_vkEndCommandBuffer    = nullptr;
    PFN_vkCmdCopyBuffer             pfn_vkCmdCopyBuffer       = nullptr;
    PFN_vkQueueSubmit               pfn_vkQueueSubmit         = nullptr;
    PFN_vkQueueWaitIdle             pfn_vkQueueWaitIdle       = nullptr;
    bool                            vk_pipeline_ready         = false;

    // --- OpenCL Dynamic State ---
    DynLibHandle                    cl_lib            = nullptr;
    cl_platform_id                  cl_platform       = nullptr;
    cl_device_id                    cl_device         = nullptr;
    cl_context                      cl_ctx            = nullptr;
    cl_command_queue                cl_queue          = nullptr;
    cl_program                      cl_prog           = nullptr;
    cl_kernel                       cl_kern_blend     = nullptr;
    cl_kernel                       cl_kern_blur_h    = nullptr;
    cl_kernel                       cl_kern_blur_v    = nullptr;
    cl_kernel                       cl_kern_ai_up     = nullptr;
    cl_mem                          cl_buf_fg         = nullptr;
    cl_mem                          cl_buf_bg         = nullptr;
    cl_mem                          cl_buf_mask       = nullptr;
    cl_mem                          cl_buf_out        = nullptr;
    cl_mem                          cl_buf_prob       = nullptr;
    size_t                          cl_rgba_cap       = 0;
    size_t                          cl_mask_cap       = 0;
    size_t                          cl_prob_cap       = 0;
    PFN_clGetPlatformIDs            pfn_clGetPlatformIDs      = nullptr;
    PFN_clGetDeviceIDs              pfn_clGetDeviceIDs        = nullptr;
    PFN_clGetDeviceInfo             pfn_clGetDeviceInfo       = nullptr;
    PFN_clCreateContext             pfn_clCreateContext       = nullptr;
    PFN_clReleaseContext            pfn_clReleaseContext      = nullptr;
    PFN_clCreateCommandQueue        pfn_clCreateCommandQueue  = nullptr;
    PFN_clCreateCommandQueueWithProperties pfn_clCreateCommandQueueWithProperties = nullptr;
    PFN_clReleaseCommandQueue       pfn_clReleaseCommandQueue = nullptr;
    PFN_clCreateProgramWithSource   pfn_clCreateProgramWithSource = nullptr;
    PFN_clBuildProgram              pfn_clBuildProgram        = nullptr;
    PFN_clReleaseProgram            pfn_clReleaseProgram      = nullptr;
    PFN_clCreateKernel              pfn_clCreateKernel        = nullptr;
    PFN_clSetKernelArg              pfn_clSetKernelArg        = nullptr;
    PFN_clEnqueueNDRangeKernel      pfn_clEnqueueNDRangeKernel = nullptr;
    PFN_clReleaseKernel             pfn_clReleaseKernel       = nullptr;
    PFN_clCreateBuffer              pfn_clCreateBuffer        = nullptr;
    PFN_clEnqueueReadBuffer         pfn_clEnqueueReadBuffer   = nullptr;
    PFN_clEnqueueWriteBuffer        pfn_clEnqueueWriteBuffer  = nullptr;
    PFN_clReleaseMemObject          pfn_clReleaseMemObject    = nullptr;
    PFN_clFinish                    pfn_clFinish              = nullptr;
    bool                            cl_kernels_ready          = false;

    // --- NVIDIA CUDA Dynamic State ---
    DynLibHandle                    cu_lib            = nullptr;
    CUdevice                        cu_device         = 0;
    CUcontext                       cu_ctx            = nullptr;
    CUdeviceptr                     cu_dptr           = 0;
    size_t                          cu_dptr_cap       = 0;
    PFN_cuInit                      pfn_cuInit                = nullptr;
    PFN_cuDriverGetVersion          pfn_cuDriverGetVersion    = nullptr;
    PFN_cuDeviceGetCount            pfn_cuDeviceGetCount      = nullptr;
    PFN_cuDeviceGet                 pfn_cuDeviceGet           = nullptr;
    PFN_cuDeviceGetName             pfn_cuDeviceGetName       = nullptr;
    PFN_cuDeviceTotalMem_v2         pfn_cuDeviceTotalMem      = nullptr;
    PFN_cuDeviceGetAttribute        pfn_cuDeviceGetAttribute  = nullptr;
    PFN_cuCtxCreate_v2              pfn_cuCtxCreate           = nullptr;
    PFN_cuCtxSetCurrent             pfn_cuCtxSetCurrent       = nullptr;
    PFN_cuCtxSynchronize            pfn_cuCtxSynchronize      = nullptr;
    PFN_cuCtxDestroy_v2             pfn_cuCtxDestroy          = nullptr;
    PFN_cuMemAlloc_v2               pfn_cuMemAlloc            = nullptr;
    PFN_cuMemFree_v2                pfn_cuMemFree             = nullptr;
    PFN_cuMemcpyHtoD_v2             pfn_cuMemcpyHtoD          = nullptr;
    PFN_cuMemcpyDtoH_v2             pfn_cuMemcpyDtoH          = nullptr;
    bool                            cu_ready                  = false;

    // --- AMD ROCm / HIP Dynamic State ---
    DynLibHandle                    hip_lib           = nullptr;
    hipDevice_t                     hip_device        = 0;
    void*                           hip_dptr          = nullptr;
    size_t                          hip_dptr_cap      = 0;
    PFN_hipInit                     pfn_hipInit               = nullptr;
    PFN_hipDriverGetVersion         pfn_hipDriverGetVersion   = nullptr;
    PFN_hipRuntimeGetVersion        pfn_hipRuntimeGetVersion  = nullptr;
    PFN_hipGetDeviceCount           pfn_hipGetDeviceCount     = nullptr;
    PFN_hipDeviceGet                pfn_hipDeviceGet          = nullptr;
    PFN_hipDeviceGetName             pfn_hipDeviceGetName      = nullptr;
    PFN_hipDeviceTotalMem           pfn_hipDeviceTotalMem     = nullptr;
    PFN_hipMemGetInfo               pfn_hipMemGetInfo         = nullptr;
    PFN_hipDeviceGetAttribute       pfn_hipDeviceGetAttribute = nullptr;
    PFN_hipMalloc                   pfn_hipMalloc             = nullptr;
    PFN_hipFree                     pfn_hipFree               = nullptr;
    PFN_hipMemcpy                   pfn_hipMemcpy             = nullptr;
    PFN_hipDeviceSynchronize        pfn_hipDeviceSynchronize  = nullptr;
    bool                            hip_ready                 = false;

    explicit RuntimeImpl(SimdTier initial_simd)
        : cpu_compositor(initial_simd) {}

    ~RuntimeImpl() {
        ShutdownAll();
    }

    template <typename FnPtr>
    bool LoadVkInstanceSym(const char* name, FnPtr& out_fn) noexcept {
        out_fn = nullptr;
        if (!pfn_vkGetInstanceProcAddr) return false;
        PFN_vkVoidFunction raw = pfn_vkGetInstanceProcAddr(vk_instance, name);
        if (!raw && vk_lib) {
            return LoadSymbol(vk_lib, name, out_fn);
        }
        if (!raw) return false;
        std::memcpy(&out_fn, &raw, sizeof(FnPtr));
        return out_fn != nullptr;
    }

    void ShutdownVulkan() noexcept {
        if (vk_device && pfn_vkQueueWaitIdle && vk_queue) {
            pfn_vkQueueWaitIdle(vk_queue);
        }
        if (vk_device && vk_mapped_src && pfn_vkUnmapMemory) {
            pfn_vkUnmapMemory(vk_device, vk_staging_src_mem);
            vk_mapped_src = nullptr;
        }
        if (vk_device && vk_mapped_dst && pfn_vkUnmapMemory) {
            pfn_vkUnmapMemory(vk_device, vk_staging_dst_mem);
            vk_mapped_dst = nullptr;
        }
        if (vk_device && vk_staging_src && pfn_vkDestroyBuffer) {
            pfn_vkDestroyBuffer(vk_device, vk_staging_src, nullptr);
            vk_staging_src = 0;
        }
        if (vk_device && vk_staging_dst && pfn_vkDestroyBuffer) {
            pfn_vkDestroyBuffer(vk_device, vk_staging_dst, nullptr);
            vk_staging_dst = 0;
        }
        if (vk_device && vk_staging_src_mem && pfn_vkFreeMemory) {
            pfn_vkFreeMemory(vk_device, vk_staging_src_mem, nullptr);
            vk_staging_src_mem = 0;
        }
        if (vk_device && vk_staging_dst_mem && pfn_vkFreeMemory) {
            pfn_vkFreeMemory(vk_device, vk_staging_dst_mem, nullptr);
            vk_staging_dst_mem = 0;
        }
        if (vk_device && vk_cmd_pool && pfn_vkDestroyCommandPool) {
            pfn_vkDestroyCommandPool(vk_device, vk_cmd_pool, nullptr);
            vk_cmd_pool = 0;
        }
        if (vk_device && pfn_vkDestroyDevice) {
            pfn_vkDestroyDevice(vk_device, nullptr);
            vk_device = nullptr;
        }
        if (vk_instance && pfn_vkDestroyInstance) {
            pfn_vkDestroyInstance(vk_instance, nullptr);
            vk_instance = nullptr;
        }
        if (vk_lib) {
            CloseDynamicLibrary(vk_lib);
            vk_lib = nullptr;
        }
        vk_pipeline_ready = false;
        vk_buffer_cap     = 0;
    }

    void ShutdownOpenCL() noexcept {
        if (cl_queue && pfn_clFinish) {
            pfn_clFinish(cl_queue);
        }
        if (pfn_clReleaseMemObject) {
            if (cl_buf_fg)   { pfn_clReleaseMemObject(cl_buf_fg);   cl_buf_fg   = nullptr; }
            if (cl_buf_bg)   { pfn_clReleaseMemObject(cl_buf_bg);   cl_buf_bg   = nullptr; }
            if (cl_buf_mask) { pfn_clReleaseMemObject(cl_buf_mask); cl_buf_mask = nullptr; }
            if (cl_buf_out)  { pfn_clReleaseMemObject(cl_buf_out);  cl_buf_out  = nullptr; }
            if (cl_buf_prob) { pfn_clReleaseMemObject(cl_buf_prob); cl_buf_prob = nullptr; }
        }
        if (pfn_clReleaseKernel) {
            if (cl_kern_blend)  { pfn_clReleaseKernel(cl_kern_blend);  cl_kern_blend  = nullptr; }
            if (cl_kern_blur_h) { pfn_clReleaseKernel(cl_kern_blur_h); cl_kern_blur_h = nullptr; }
            if (cl_kern_blur_v) { pfn_clReleaseKernel(cl_kern_blur_v); cl_kern_blur_v = nullptr; }
            if (cl_kern_ai_up)  { pfn_clReleaseKernel(cl_kern_ai_up);  cl_kern_ai_up  = nullptr; }
        }
        if (cl_prog && pfn_clReleaseProgram) {
            pfn_clReleaseProgram(cl_prog);
            cl_prog = nullptr;
        }
        if (cl_queue && pfn_clReleaseCommandQueue) {
            pfn_clReleaseCommandQueue(cl_queue);
            cl_queue = nullptr;
        }
        if (cl_ctx && pfn_clReleaseContext) {
            pfn_clReleaseContext(cl_ctx);
            cl_ctx = nullptr;
        }
        if (cl_lib) {
            CloseDynamicLibrary(cl_lib);
            cl_lib = nullptr;
        }
        cl_kernels_ready = false;
        cl_rgba_cap      = 0;
        cl_mask_cap      = 0;
        cl_prob_cap      = 0;
    }

    void ShutdownCUDA() noexcept {
        if (cu_ctx && pfn_cuCtxSetCurrent) {
            pfn_cuCtxSetCurrent(cu_ctx);
        }
        if (cu_dptr && pfn_cuMemFree) {
            pfn_cuMemFree(cu_dptr);
            cu_dptr = 0;
        }
        if (cu_ctx && pfn_cuCtxDestroy) {
            pfn_cuCtxDestroy(cu_ctx);
            cu_ctx = nullptr;
        }
        if (cu_lib) {
            CloseDynamicLibrary(cu_lib);
            cu_lib = nullptr;
        }
        cu_ready    = false;
        cu_dptr_cap = 0;
    }

    void ShutdownROCm() noexcept {
        if (hip_dptr && pfn_hipFree) {
            pfn_hipFree(hip_dptr);
            hip_dptr = nullptr;
        }
        if (hip_lib) {
            CloseDynamicLibrary(hip_lib);
            hip_lib = nullptr;
        }
        hip_ready    = false;
        hip_dptr_cap = 0;
    }

    void ShutdownAll() noexcept {
        ShutdownOpenCL();
        ShutdownVulkan();
        ShutdownCUDA();
        ShutdownROCm();
    }

    // ------------------------------------------------------------------------
    // 1. CPU Backend Probing
    // ------------------------------------------------------------------------
    ComputeDeviceInfo ProbeCpuBackend() noexcept {
        ComputeDeviceInfo info{};
        info.backend = ComputeBackend::CPU;
        CopyFixedString(info.backend_name, sizeof(info.backend_name), "CPU");
        QueryCpuBrandString(info.device_name, sizeof(info.device_name));

        const SimdTier best_simd = SimdCompositor::DetectBestSimdTier();
        const char* simd_str = (best_simd == SimdTier::AVX512) ? "AVX-512"
                             : (best_simd == SimdTier::AVX2)   ? "AVX2"
                                                               : "Scalar";
        char drv_buf[64] = {0};
        std::snprintf(drv_buf, sizeof(drv_buf), "Native C++17 SIMD (%s)", simd_str);
        CopyFixedString(info.driver_version, sizeof(info.driver_version), drv_buf);

        info.vram_bytes        = QuerySystemRamBytes();
        const unsigned int hc  = std::thread::hardware_concurrency();
        info.compute_units     = (hc > 0) ? static_cast<uint32_t>(hc) : 4u;
        info.available         = true;
        info.is_integrated_gpu = false;
        return info;
    }

    // ------------------------------------------------------------------------
    // 2. Vulkan Backend Probing & Runtime Setup
    // ------------------------------------------------------------------------
    ComputeDeviceInfo ProbeVulkanBackend() noexcept {
        ShutdownVulkan();

        ComputeDeviceInfo info{};
        info.backend = ComputeBackend::VULKAN;
        CopyFixedString(info.backend_name, sizeof(info.backend_name), "Vulkan");
        CopyFixedString(info.device_name, sizeof(info.device_name), "Vulkan Compute Not Detected");
        CopyFixedString(info.driver_version, sizeof(info.driver_version), "N/A");
        info.available = false;

#if defined(_WIN32)
        const char* const kVkLibs[] = {"vulkan-1.dll"};
#elif defined(__APPLE__)
        const char* const kVkLibs[] = {"libvulkan.1.dylib", "libvulkan.dylib", "libMoltenVK.dylib"};
#else
        const char* const kVkLibs[] = {"libvulkan.so.1", "libvulkan.so"};
#endif
        vk_lib = OpenDynamicLibrary(kVkLibs, sizeof(kVkLibs) / sizeof(kVkLibs[0]));
        if (!vk_lib) {
            return info;
        }

        if (!LoadSymbol(vk_lib, "vkGetInstanceProcAddr", pfn_vkGetInstanceProcAddr)) {
            ShutdownVulkan();
            return info;
        }

        LoadVkInstanceSym("vkCreateInstance", pfn_vkCreateInstance);
        if (!pfn_vkCreateInstance) {
            ShutdownVulkan();
            return info;
        }

        VkApplicationInfo_Min app_info{};
        app_info.sType              = VK_STRUCTURE_TYPE_APPLICATION_INFO_VAL;
        app_info.pApplicationName   = "SmartCam Virtual Camera";
        app_info.applicationVersion = (1u << 22);
        app_info.pEngineName        = "SmartCamEngine";
        app_info.engineVersion      = (1u << 22);
        app_info.apiVersion         = (1u << 22); // Vulkan 1.0

        VkInstanceCreateInfo_Min inst_info{};
        inst_info.sType            = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO_VAL;
        inst_info.pApplicationInfo = &app_info;

        if (pfn_vkCreateInstance(&inst_info, nullptr, &vk_instance) != VK_SUCCESS_VAL || !vk_instance) {
            ShutdownVulkan();
            return info;
        }

        LoadVkInstanceSym("vkDestroyInstance", pfn_vkDestroyInstance);
        LoadVkInstanceSym("vkEnumeratePhysicalDevices", pfn_vkEnumeratePhysicalDevices);
        LoadVkInstanceSym("vkGetPhysicalDeviceProperties", pfn_vkGetPhysicalDeviceProperties);
        LoadVkInstanceSym("vkGetPhysicalDeviceMemoryProperties", pfn_vkGetPhysicalDeviceMemoryProperties);
        LoadVkInstanceSym("vkGetPhysicalDeviceQueueFamilyProperties", pfn_vkGetPhysicalDeviceQueueFamilyProperties);
        LoadVkInstanceSym("vkCreateDevice", pfn_vkCreateDevice);
        LoadVkInstanceSym("vkDestroyDevice", pfn_vkDestroyDevice);
        LoadVkInstanceSym("vkGetDeviceQueue", pfn_vkGetDeviceQueue);
        LoadVkInstanceSym("vkCreateBuffer", pfn_vkCreateBuffer);
        LoadVkInstanceSym("vkDestroyBuffer", pfn_vkDestroyBuffer);
        LoadVkInstanceSym("vkGetBufferMemoryRequirements", pfn_vkGetBufferMemoryRequirements);
        LoadVkInstanceSym("vkAllocateMemory", pfn_vkAllocateMemory);
        LoadVkInstanceSym("vkFreeMemory", pfn_vkFreeMemory);
        LoadVkInstanceSym("vkBindBufferMemory", pfn_vkBindBufferMemory);
        LoadVkInstanceSym("vkMapMemory", pfn_vkMapMemory);
        LoadVkInstanceSym("vkUnmapMemory", pfn_vkUnmapMemory);
        LoadVkInstanceSym("vkCreateCommandPool", pfn_vkCreateCommandPool);
        LoadVkInstanceSym("vkDestroyCommandPool", pfn_vkDestroyCommandPool);
        LoadVkInstanceSym("vkAllocateCommandBuffers", pfn_vkAllocateCommandBuffers);
        LoadVkInstanceSym("vkBeginCommandBuffer", pfn_vkBeginCommandBuffer);
        LoadVkInstanceSym("vkEndCommandBuffer", pfn_vkEndCommandBuffer);
        LoadVkInstanceSym("vkCmdCopyBuffer", pfn_vkCmdCopyBuffer);
        LoadVkInstanceSym("vkQueueSubmit", pfn_vkQueueSubmit);
        LoadVkInstanceSym("vkQueueWaitIdle", pfn_vkQueueWaitIdle);

        if (!pfn_vkEnumeratePhysicalDevices || !pfn_vkGetPhysicalDeviceProperties ||
            !pfn_vkGetPhysicalDeviceQueueFamilyProperties) {
            ShutdownVulkan();
            return info;
        }

        uint32_t dev_count = 0;
        if (pfn_vkEnumeratePhysicalDevices(vk_instance, &dev_count, nullptr) != VK_SUCCESS_VAL || dev_count == 0) {
            ShutdownVulkan();
            return info;
        }

        std::vector<VkPhysicalDevice> phys_devs(std::min(dev_count, 16u));
        dev_count = static_cast<uint32_t>(phys_devs.size());
        if (pfn_vkEnumeratePhysicalDevices(vk_instance, &dev_count, phys_devs.data()) != VK_SUCCESS_VAL || dev_count == 0) {
            ShutdownVulkan();
            return info;
        }

        int best_rank = -1;
        VkPhysicalDeviceProperties_Min best_props{};
        uint64_t best_vram = 0;
        uint32_t best_compute_q_fam = 0;
        uint32_t best_compute_queues = 0;
        uint32_t best_host_mem_type = UINT32_MAX;

        for (uint32_t i = 0; i < dev_count; ++i) {
            VkPhysicalDeviceProperties_Min props{};
            pfn_vkGetPhysicalDeviceProperties(phys_devs[i], &props);

            uint32_t qf_count = 0;
            pfn_vkGetPhysicalDeviceQueueFamilyProperties(phys_devs[i], &qf_count, nullptr);
            if (qf_count == 0) continue;

            std::vector<VkQueueFamilyProperties_Min> qf_props(std::min(qf_count, 32u));
            qf_count = static_cast<uint32_t>(qf_props.size());
            pfn_vkGetPhysicalDeviceQueueFamilyProperties(phys_devs[i], &qf_count, qf_props.data());

            int compute_q_idx = -1;
            uint32_t compute_q_count = 0;
            for (uint32_t q = 0; q < qf_count; ++q) {
                if ((qf_props[q].queueFlags & VK_QUEUE_COMPUTE_BIT_VAL) != 0) {
                    compute_q_idx = static_cast<int>(q);
                    compute_q_count = qf_props[q].queueCount;
                    break;
                }
            }
            if (compute_q_idx < 0) continue;

            uint64_t vram = 0;
            uint32_t host_mem_idx = UINT32_MAX;
            if (pfn_vkGetPhysicalDeviceMemoryProperties) {
                VkPhysicalDeviceMemoryProperties_Min mem_props{};
                pfn_vkGetPhysicalDeviceMemoryProperties(phys_devs[i], &mem_props);
                for (uint32_t h = 0; h < std::min(mem_props.memoryHeapCount, 16u); ++h) {
                    if ((mem_props.memoryHeaps[h].flags & VK_MEMORY_HEAP_DEVICE_LOCAL_BIT_VAL) != 0) {
                        vram = std::max<uint64_t>(vram, mem_props.memoryHeaps[h].size);
                    }
                }
                const VkFlags needed = VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT_VAL | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT_VAL;
                for (uint32_t m = 0; m < std::min(mem_props.memoryTypeCount, 32u); ++m) {
                    if ((mem_props.memoryTypes[m].propertyFlags & needed) == needed) {
                        host_mem_idx = m;
                        break;
                    }
                }
            }

            int rank = 1;
            if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU_VAL) {
                rank = 3;
            } else if (props.deviceType == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU_VAL) {
                rank = 2;
            }

            if (rank > best_rank) {
                best_rank           = rank;
                vk_phys_dev         = phys_devs[i];
                best_props          = props;
                best_vram           = vram;
                best_compute_q_fam  = static_cast<uint32_t>(compute_q_idx);
                best_compute_queues = compute_q_count;
                best_host_mem_type  = host_mem_idx;
            }
        }

        if (best_rank < 0 || !vk_phys_dev) {
            ShutdownVulkan();
            return info;
        }

        vk_compute_q_fam = best_compute_q_fam;
        vk_host_mem_type = best_host_mem_type;

        CopyFixedString(info.device_name, sizeof(info.device_name), best_props.deviceName);
        const uint32_t api_maj = (best_props.apiVersion >> 22) & 0x7Fu;
        const uint32_t api_min = (best_props.apiVersion >> 12) & 0x3FFu;
        const uint32_t api_pat = best_props.apiVersion & 0xFFFu;
        char ver_buf[64] = {0};
        std::snprintf(ver_buf, sizeof(ver_buf), "Vulkan %u.%u.%u (Drv 0x%X)",
                      api_maj, api_min, api_pat, best_props.driverVersion);
        CopyFixedString(info.driver_version, sizeof(info.driver_version), ver_buf);

        info.vram_bytes        = (best_vram > 0) ? best_vram : (QuerySystemRamBytes() / 2);
        info.compute_units     = std::max(1u, best_compute_queues);
        info.available         = true;
        info.is_integrated_gpu = (best_props.deviceType == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU_VAL) ||
                                 InferIntegratedGpuFromName(best_props.deviceName, nullptr);

        // Initialize logical compute device & command queue for DMA / compute submissions
        if (pfn_vkCreateDevice && pfn_vkGetDeviceQueue && pfn_vkCreateCommandPool &&
            pfn_vkAllocateCommandBuffers && vk_host_mem_type != UINT32_MAX) {
            const float priority = 1.0f;
            VkDeviceQueueCreateInfo_Min q_info{};
            q_info.sType            = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO_VAL;
            q_info.queueFamilyIndex = vk_compute_q_fam;
            q_info.queueCount       = 1;
            q_info.pQueuePriorities = &priority;

            VkDeviceCreateInfo_Min dev_info{};
            dev_info.sType                = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO_VAL;
            dev_info.queueCreateInfoCount = 1;
            dev_info.pQueueCreateInfos    = &q_info;

            if (pfn_vkCreateDevice(vk_phys_dev, &dev_info, nullptr, &vk_device) == VK_SUCCESS_VAL && vk_device) {
                pfn_vkGetDeviceQueue(vk_device, vk_compute_q_fam, 0, &vk_queue);

                VkCommandPoolCreateInfo_Min pool_info{};
                pool_info.sType            = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO_VAL;
                pool_info.flags            = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT_VAL;
                pool_info.queueFamilyIndex = vk_compute_q_fam;

                if (pfn_vkCreateCommandPool(vk_device, &pool_info, nullptr, &vk_cmd_pool) == VK_SUCCESS_VAL) {
                    VkCommandBufferAllocateInfo_Min cb_alloc{};
                    cb_alloc.sType              = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO_VAL;
                    cb_alloc.commandPool        = vk_cmd_pool;
                    cb_alloc.level              = 0;
                    cb_alloc.commandBufferCount = 1;
                    if (pfn_vkAllocateCommandBuffers(vk_device, &cb_alloc, &vk_cmd_buf) == VK_SUCCESS_VAL && vk_cmd_buf) {
                        vk_pipeline_ready = true;
                    }
                }
            }
        }

        return info;
    }

    bool EnsureVulkanBuffers(size_t byte_size) noexcept {
        if (!vk_pipeline_ready || !vk_device || byte_size == 0) return false;
        if (vk_buffer_cap >= byte_size && vk_mapped_src && vk_mapped_dst) return true;

        if (!pfn_vkCreateBuffer || !pfn_vkGetBufferMemoryRequirements ||
            !pfn_vkAllocateMemory || !pfn_vkBindBufferMemory || !pfn_vkMapMemory) {
            return false;
        }

        if (vk_mapped_src && pfn_vkUnmapMemory) { pfn_vkUnmapMemory(vk_device, vk_staging_src_mem); vk_mapped_src = nullptr; }
        if (vk_mapped_dst && pfn_vkUnmapMemory) { pfn_vkUnmapMemory(vk_device, vk_staging_dst_mem); vk_mapped_dst = nullptr; }
        if (vk_staging_src && pfn_vkDestroyBuffer) { pfn_vkDestroyBuffer(vk_device, vk_staging_src, nullptr); vk_staging_src = 0; }
        if (vk_staging_dst && pfn_vkDestroyBuffer) { pfn_vkDestroyBuffer(vk_device, vk_staging_dst, nullptr); vk_staging_dst = 0; }
        if (vk_staging_src_mem && pfn_vkFreeMemory) { pfn_vkFreeMemory(vk_device, vk_staging_src_mem, nullptr); vk_staging_src_mem = 0; }
        if (vk_staging_dst_mem && pfn_vkFreeMemory) { pfn_vkFreeMemory(vk_device, vk_staging_dst_mem, nullptr); vk_staging_dst_mem = 0; }

        VkBufferCreateInfo_Min buf_info{};
        buf_info.sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO_VAL;
        buf_info.size  = static_cast<VkDeviceSize>(byte_size);
        buf_info.usage = VK_BUFFER_USAGE_TRANSFER_SRC_BIT_VAL | VK_BUFFER_USAGE_TRANSFER_DST_BIT_VAL | VK_BUFFER_USAGE_STORAGE_BUFFER_BIT_VAL;

        if (pfn_vkCreateBuffer(vk_device, &buf_info, nullptr, &vk_staging_src) != VK_SUCCESS_VAL ||
            pfn_vkCreateBuffer(vk_device, &buf_info, nullptr, &vk_staging_dst) != VK_SUCCESS_VAL) {
            return false;
        }

        VkMemoryRequirements_Min req_src{}, req_dst{};
        pfn_vkGetBufferMemoryRequirements(vk_device, vk_staging_src, &req_src);
        pfn_vkGetBufferMemoryRequirements(vk_device, vk_staging_dst, &req_dst);

        VkMemoryAllocateInfo_Min alloc_src{};
        alloc_src.sType           = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO_VAL;
        alloc_src.allocationSize  = std::max<VkDeviceSize>(req_src.size, byte_size);
        alloc_src.memoryTypeIndex = vk_host_mem_type;

        VkMemoryAllocateInfo_Min alloc_dst{};
        alloc_dst.sType           = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO_VAL;
        alloc_dst.allocationSize  = std::max<VkDeviceSize>(req_dst.size, byte_size);
        alloc_dst.memoryTypeIndex = vk_host_mem_type;

        if (pfn_vkAllocateMemory(vk_device, &alloc_src, nullptr, &vk_staging_src_mem) != VK_SUCCESS_VAL ||
            pfn_vkAllocateMemory(vk_device, &alloc_dst, nullptr, &vk_staging_dst_mem) != VK_SUCCESS_VAL) {
            return false;
        }

        if (pfn_vkBindBufferMemory(vk_device, vk_staging_src, vk_staging_src_mem, 0) != VK_SUCCESS_VAL ||
            pfn_vkBindBufferMemory(vk_device, vk_staging_dst, vk_staging_dst_mem, 0) != VK_SUCCESS_VAL) {
            return false;
        }

        if (pfn_vkMapMemory(vk_device, vk_staging_src_mem, 0, byte_size, 0, &vk_mapped_src) != VK_SUCCESS_VAL ||
            pfn_vkMapMemory(vk_device, vk_staging_dst_mem, 0, byte_size, 0, &vk_mapped_dst) != VK_SUCCESS_VAL) {
            return false;
        }

        vk_buffer_cap = byte_size;
        return true;
    }

    bool ExecuteVulkanDmaRoundtrip(const void* src_data, void* dst_data, size_t byte_size) noexcept {
        if (!EnsureVulkanBuffers(byte_size) || !pfn_vkBeginCommandBuffer || !pfn_vkCmdCopyBuffer ||
            !pfn_vkEndCommandBuffer || !pfn_vkQueueSubmit || !pfn_vkQueueWaitIdle) {
            return false;
        }

        std::memcpy(vk_mapped_src, src_data, byte_size);

        VkCommandBufferBeginInfo_Min begin_info{};
        begin_info.sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO_VAL;
        begin_info.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT_VAL;

        if (pfn_vkBeginCommandBuffer(vk_cmd_buf, &begin_info) != VK_SUCCESS_VAL) {
            return false;
        }

        VkBufferCopy_Min copy_region{};
        copy_region.srcOffset = 0;
        copy_region.dstOffset = 0;
        copy_region.size      = static_cast<VkDeviceSize>(byte_size);
        pfn_vkCmdCopyBuffer(vk_cmd_buf, vk_staging_src, vk_staging_dst, 1, &copy_region);

        if (pfn_vkEndCommandBuffer(vk_cmd_buf) != VK_SUCCESS_VAL) {
            return false;
        }

        VkSubmitInfo_Min submit{};
        submit.sType              = VK_STRUCTURE_TYPE_SUBMIT_INFO_VAL;
        submit.commandBufferCount = 1;
        submit.pCommandBuffers    = &vk_cmd_buf;

        if (pfn_vkQueueSubmit(vk_queue, 1, &submit, 0) != VK_SUCCESS_VAL) {
            return false;
        }
        if (pfn_vkQueueWaitIdle(vk_queue) != VK_SUCCESS_VAL) {
            return false;
        }

        if (dst_data) {
            std::memcpy(dst_data, vk_mapped_dst, byte_size);
        }
        return true;
    }

    // ------------------------------------------------------------------------
    // 3. OpenCL Backend Probing & Runtime Kernel Compilation
    // ------------------------------------------------------------------------
    ComputeDeviceInfo ProbeOpenCLBackend() noexcept {
        ShutdownOpenCL();

        ComputeDeviceInfo info{};
        info.backend = ComputeBackend::OPENCL;
        CopyFixedString(info.backend_name, sizeof(info.backend_name), "OpenCL");
        CopyFixedString(info.device_name, sizeof(info.device_name), "OpenCL Compute Not Detected");
        CopyFixedString(info.driver_version, sizeof(info.driver_version), "N/A");
        info.available = false;

#if defined(_WIN32)
        const char* const kClLibs[] = {"OpenCL.dll"};
#elif defined(__APPLE__)
        const char* const kClLibs[] = {"/System/Library/Frameworks/OpenCL.framework/OpenCL"};
#else
        const char* const kClLibs[] = {"libOpenCL.so.1", "libOpenCL.so"};
#endif
        cl_lib = OpenDynamicLibrary(kClLibs, sizeof(kClLibs) / sizeof(kClLibs[0]));
        if (!cl_lib) {
            return info;
        }

        if (!LoadSymbol(cl_lib, "clGetPlatformIDs", pfn_clGetPlatformIDs) ||
            !LoadSymbol(cl_lib, "clGetDeviceIDs", pfn_clGetDeviceIDs) ||
            !LoadSymbol(cl_lib, "clGetDeviceInfo", pfn_clGetDeviceInfo) ||
            !LoadSymbol(cl_lib, "clCreateContext", pfn_clCreateContext) ||
            !LoadSymbol(cl_lib, "clReleaseContext", pfn_clReleaseContext) ||
            !LoadSymbol(cl_lib, "clReleaseCommandQueue", pfn_clReleaseCommandQueue) ||
            !LoadSymbol(cl_lib, "clCreateProgramWithSource", pfn_clCreateProgramWithSource) ||
            !LoadSymbol(cl_lib, "clBuildProgram", pfn_clBuildProgram) ||
            !LoadSymbol(cl_lib, "clReleaseProgram", pfn_clReleaseProgram) ||
            !LoadSymbol(cl_lib, "clCreateKernel", pfn_clCreateKernel) ||
            !LoadSymbol(cl_lib, "clSetKernelArg", pfn_clSetKernelArg) ||
            !LoadSymbol(cl_lib, "clEnqueueNDRangeKernel", pfn_clEnqueueNDRangeKernel) ||
            !LoadSymbol(cl_lib, "clReleaseKernel", pfn_clReleaseKernel) ||
            !LoadSymbol(cl_lib, "clCreateBuffer", pfn_clCreateBuffer) ||
            !LoadSymbol(cl_lib, "clEnqueueReadBuffer", pfn_clEnqueueReadBuffer) ||
            !LoadSymbol(cl_lib, "clEnqueueWriteBuffer", pfn_clEnqueueWriteBuffer) ||
            !LoadSymbol(cl_lib, "clReleaseMemObject", pfn_clReleaseMemObject) ||
            !LoadSymbol(cl_lib, "clFinish", pfn_clFinish)) {
            ShutdownOpenCL();
            return info;
        }
        LoadSymbol(cl_lib, "clCreateCommandQueue", pfn_clCreateCommandQueue);
        LoadSymbol(cl_lib, "clCreateCommandQueueWithProperties", pfn_clCreateCommandQueueWithProperties);

        cl_uint num_platforms = 0;
        if (pfn_clGetPlatformIDs(0, nullptr, &num_platforms) != CL_SUCCESS_VAL || num_platforms == 0) {
            ShutdownOpenCL();
            return info;
        }

        std::vector<cl_platform_id> platforms(std::min(num_platforms, 16u));
        num_platforms = static_cast<cl_uint>(platforms.size());
        if (pfn_clGetPlatformIDs(num_platforms, platforms.data(), nullptr) != CL_SUCCESS_VAL) {
            ShutdownOpenCL();
            return info;
        }

        // First search for GPU devices across all OpenCL platforms; fall back to CL_DEVICE_TYPE_ALL
        for (int pass = 0; pass < 2 && !cl_device; ++pass) {
            const cl_device_type dtype = (pass == 0) ? CL_DEVICE_TYPE_GPU_VAL : CL_DEVICE_TYPE_ALL_VAL;
            for (cl_uint p = 0; p < num_platforms; ++p) {
                cl_uint num_devs = 0;
                if (pfn_clGetDeviceIDs(platforms[p], dtype, 0, nullptr, &num_devs) == CL_SUCCESS_VAL && num_devs > 0) {
                    cl_device_id candidate = nullptr;
                    if (pfn_clGetDeviceIDs(platforms[p], dtype, 1, &candidate, nullptr) == CL_SUCCESS_VAL && candidate) {
                        cl_platform = platforms[p];
                        cl_device   = candidate;
                        break;
                    }
                }
            }
        }

        if (!cl_device) {
            ShutdownOpenCL();
            return info;
        }

        char dev_name[128]    = {0};
        char vendor_name[128] = {0};
        char drv_ver[64]      = {0};
        cl_ulong global_mem   = 0;
        cl_uint compute_units = 0;
        cl_bool unified_mem   = 0;

        pfn_clGetDeviceInfo(cl_device, CL_DEVICE_NAME_VAL, sizeof(dev_name) - 1, dev_name, nullptr);
        pfn_clGetDeviceInfo(cl_device, CL_DEVICE_VENDOR_VAL, sizeof(vendor_name) - 1, vendor_name, nullptr);
        if (pfn_clGetDeviceInfo(cl_device, CL_DRIVER_VERSION_VAL, sizeof(drv_ver) - 1, drv_ver, nullptr) != CL_SUCCESS_VAL || drv_ver[0] == '\0') {
            pfn_clGetDeviceInfo(cl_device, CL_DEVICE_VERSION_VAL, sizeof(drv_ver) - 1, drv_ver, nullptr);
        }
        pfn_clGetDeviceInfo(cl_device, CL_DEVICE_GLOBAL_MEM_SIZE_VAL, sizeof(global_mem), &global_mem, nullptr);
        pfn_clGetDeviceInfo(cl_device, CL_DEVICE_MAX_COMPUTE_UNITS_VAL, sizeof(compute_units), &compute_units, nullptr);
        pfn_clGetDeviceInfo(cl_device, CL_DEVICE_HOST_UNIFIED_MEMORY_VAL, sizeof(unified_mem), &unified_mem, nullptr);

        TrimAsciiWhitespace(dev_name);
        TrimAsciiWhitespace(vendor_name);
        TrimAsciiWhitespace(drv_ver);

        CopyFixedString(info.device_name, sizeof(info.device_name),
                        dev_name[0] ? dev_name : "OpenCL Compatible GPU");
        CopyFixedString(info.driver_version, sizeof(info.driver_version),
                        drv_ver[0] ? drv_ver : "OpenCL 1.2+");
        info.vram_bytes        = static_cast<uint64_t>(global_mem);
        info.compute_units     = std::max(1u, static_cast<uint32_t>(compute_units));
        info.available         = true;
        info.is_integrated_gpu = (unified_mem != 0) || InferIntegratedGpuFromName(dev_name, vendor_name);

        // Create OpenCL context, command queue, and compile runtime kernels
        cl_int err = 0;
        cl_ctx = pfn_clCreateContext(nullptr, 1, &cl_device, nullptr, nullptr, &err);
        if (err == CL_SUCCESS_VAL && cl_ctx) {
            if (pfn_clCreateCommandQueueWithProperties) {
                cl_queue = pfn_clCreateCommandQueueWithProperties(cl_ctx, cl_device, nullptr, &err);
            }
            if (!cl_queue && pfn_clCreateCommandQueue) {
                cl_queue = pfn_clCreateCommandQueue(cl_ctx, cl_device, 0, &err);
            }

            if (cl_queue) {
                const char* src_ptr = kOpenClKernelSource;
                const size_t src_len = std::strlen(kOpenClKernelSource);
                cl_prog = pfn_clCreateProgramWithSource(cl_ctx, 1, &src_ptr, &src_len, &err);
                if (err == CL_SUCCESS_VAL && cl_prog) {
                    if (pfn_clBuildProgram(cl_prog, 1, &cl_device, "-cl-fast-relaxed-math", nullptr, nullptr) == CL_SUCCESS_VAL) {
                        cl_kern_blend  = pfn_clCreateKernel(cl_prog, "smartcam_blend_bgra", &err);
                        cl_kern_blur_h = pfn_clCreateKernel(cl_prog, "smartcam_box_blur_h", &err);
                        cl_kern_blur_v = pfn_clCreateKernel(cl_prog, "smartcam_box_blur_v", &err);
                        cl_kern_ai_up  = pfn_clCreateKernel(cl_prog, "smartcam_ai_upsample", &err);
                        if (cl_kern_blend && cl_kern_blur_h && cl_kern_blur_v && cl_kern_ai_up) {
                            cl_kernels_ready = true;
                        }
                    }
                }
            }
        }

        return info;
    }

    bool EnsureOpenCLBuffers(size_t rgba_bytes, size_t mask_bytes, size_t prob_bytes) noexcept {
        if (!cl_kernels_ready || !cl_ctx || !cl_queue) return false;
        cl_int err = 0;

        if (rgba_bytes > cl_rgba_cap) {
            if (cl_buf_fg)  { pfn_clReleaseMemObject(cl_buf_fg);  cl_buf_fg  = nullptr; }
            if (cl_buf_bg)  { pfn_clReleaseMemObject(cl_buf_bg);  cl_buf_bg  = nullptr; }
            if (cl_buf_out) { pfn_clReleaseMemObject(cl_buf_out); cl_buf_out = nullptr; }

            cl_buf_fg  = pfn_clCreateBuffer(cl_ctx, CL_MEM_READ_WRITE_VAL, rgba_bytes, nullptr, &err);
            cl_buf_bg  = pfn_clCreateBuffer(cl_ctx, CL_MEM_READ_WRITE_VAL, rgba_bytes, nullptr, &err);
            cl_buf_out = pfn_clCreateBuffer(cl_ctx, CL_MEM_READ_WRITE_VAL, rgba_bytes, nullptr, &err);
            if (!cl_buf_fg || !cl_buf_bg || !cl_buf_out) {
                cl_rgba_cap = 0;
                return false;
            }
            cl_rgba_cap = rgba_bytes;
        }

        if (mask_bytes > cl_mask_cap && mask_bytes > 0) {
            if (cl_buf_mask) { pfn_clReleaseMemObject(cl_buf_mask); cl_buf_mask = nullptr; }
            cl_buf_mask = pfn_clCreateBuffer(cl_ctx, CL_MEM_READ_WRITE_VAL, mask_bytes, nullptr, &err);
            if (!cl_buf_mask) {
                cl_mask_cap = 0;
                return false;
            }
            cl_mask_cap = mask_bytes;
        }

        if (prob_bytes > cl_prob_cap && prob_bytes > 0) {
            if (cl_buf_prob) { pfn_clReleaseMemObject(cl_buf_prob); cl_buf_prob = nullptr; }
            cl_buf_prob = pfn_clCreateBuffer(cl_ctx, CL_MEM_READ_ONLY_VAL, prob_bytes, nullptr, &err);
            if (!cl_buf_prob) {
                cl_prob_cap = 0;
                return false;
            }
            cl_prob_cap = prob_bytes;
        }

        return true;
    }

    // ------------------------------------------------------------------------
    // 4. NVIDIA CUDA Driver API Probing & Runtime Setup
    // ------------------------------------------------------------------------
    ComputeDeviceInfo ProbeCudaBackend() noexcept {
        ShutdownCUDA();

        ComputeDeviceInfo info{};
        info.backend = ComputeBackend::CUDA;
        CopyFixedString(info.backend_name, sizeof(info.backend_name), "CUDA");
        CopyFixedString(info.device_name, sizeof(info.device_name), "NVIDIA CUDA Not Detected");
        CopyFixedString(info.driver_version, sizeof(info.driver_version), "N/A");
        info.available = false;

#if defined(_WIN32)
        const char* const kCudaLibs[] = {"nvcuda.dll"};
#else
        const char* const kCudaLibs[] = {"libcuda.so.1", "libcuda.so"};
#endif
        cu_lib = OpenDynamicLibrary(kCudaLibs, sizeof(kCudaLibs) / sizeof(kCudaLibs[0]));
        if (!cu_lib) {
            return info;
        }

        if (!LoadSymbol(cu_lib, "cuInit", pfn_cuInit) ||
            !LoadSymbol(cu_lib, "cuDeviceGetCount", pfn_cuDeviceGetCount) ||
            !LoadSymbol(cu_lib, "cuDeviceGet", pfn_cuDeviceGet) ||
            !LoadSymbol(cu_lib, "cuDeviceGetName", pfn_cuDeviceGetName)) {
            ShutdownCUDA();
            return info;
        }

        LoadSymbol(cu_lib, "cuDriverGetVersion", pfn_cuDriverGetVersion);
        if (!LoadSymbol(cu_lib, "cuDeviceTotalMem_v2", pfn_cuDeviceTotalMem)) {
            LoadSymbol(cu_lib, "cuDeviceTotalMem", pfn_cuDeviceTotalMem);
        }
        LoadSymbol(cu_lib, "cuDeviceGetAttribute", pfn_cuDeviceGetAttribute);
        if (!LoadSymbol(cu_lib, "cuCtxCreate_v2", pfn_cuCtxCreate)) {
            LoadSymbol(cu_lib, "cuCtxCreate", pfn_cuCtxCreate);
        }
        LoadSymbol(cu_lib, "cuCtxSetCurrent", pfn_cuCtxSetCurrent);
        LoadSymbol(cu_lib, "cuCtxSynchronize", pfn_cuCtxSynchronize);
        if (!LoadSymbol(cu_lib, "cuCtxDestroy_v2", pfn_cuCtxDestroy)) {
            LoadSymbol(cu_lib, "cuCtxDestroy", pfn_cuCtxDestroy);
        }
        if (!LoadSymbol(cu_lib, "cuMemAlloc_v2", pfn_cuMemAlloc)) {
            LoadSymbol(cu_lib, "cuMemAlloc", pfn_cuMemAlloc);
        }
        if (!LoadSymbol(cu_lib, "cuMemFree_v2", pfn_cuMemFree)) {
            LoadSymbol(cu_lib, "cuMemFree", pfn_cuMemFree);
        }
        if (!LoadSymbol(cu_lib, "cuMemcpyHtoD_v2", pfn_cuMemcpyHtoD)) {
            LoadSymbol(cu_lib, "cuMemcpyHtoD", pfn_cuMemcpyHtoD);
        }
        if (!LoadSymbol(cu_lib, "cuMemcpyDtoH_v2", pfn_cuMemcpyDtoH)) {
            LoadSymbol(cu_lib, "cuMemcpyDtoH", pfn_cuMemcpyDtoH);
        }

        if (pfn_cuInit(0) != CUDA_SUCCESS_VAL) {
            ShutdownCUDA();
            return info;
        }

        int32_t dev_count = 0;
        if (pfn_cuDeviceGetCount(&dev_count) != CUDA_SUCCESS_VAL || dev_count <= 0) {
            ShutdownCUDA();
            return info;
        }

        if (pfn_cuDeviceGet(&cu_device, 0) != CUDA_SUCCESS_VAL) {
            ShutdownCUDA();
            return info;
        }

        char dev_name[128] = {0};
        pfn_cuDeviceGetName(dev_name, sizeof(dev_name) - 1, cu_device);
        TrimAsciiWhitespace(dev_name);
        CopyFixedString(info.device_name, sizeof(info.device_name),
                        dev_name[0] ? dev_name : "NVIDIA CUDA GPU");

        size_t total_mem = 0;
        if (pfn_cuDeviceTotalMem) {
            pfn_cuDeviceTotalMem(&total_mem, cu_device);
        }
        info.vram_bytes = static_cast<uint64_t>(total_mem);

        int32_t sm_count   = 1;
        int32_t integrated = 0;
        int32_t cc_major   = 0;
        int32_t cc_minor   = 0;
        if (pfn_cuDeviceGetAttribute) {
            pfn_cuDeviceGetAttribute(&sm_count, CU_DEVICE_ATTRIBUTE_MULTIPROCESSOR_COUNT_VAL, cu_device);
            pfn_cuDeviceGetAttribute(&integrated, CU_DEVICE_ATTRIBUTE_INTEGRATED_VAL, cu_device);
            pfn_cuDeviceGetAttribute(&cc_major, CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR_VAL, cu_device);
            pfn_cuDeviceGetAttribute(&cc_minor, CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR_VAL, cu_device);
        }

        int32_t drv_ver = 0;
        if (pfn_cuDriverGetVersion) {
            pfn_cuDriverGetVersion(&drv_ver);
        }
        char ver_buf[64] = {0};
        if (drv_ver > 0) {
            std::snprintf(ver_buf, sizeof(ver_buf), "CUDA %d.%d (SM %d.%d)",
                          drv_ver / 1000, (drv_ver % 100) / 10, cc_major, cc_minor);
        } else {
            std::snprintf(ver_buf, sizeof(ver_buf), "CUDA Driver (SM %d.%d)", cc_major, cc_minor);
        }
        CopyFixedString(info.driver_version, sizeof(info.driver_version), ver_buf);

        info.compute_units     = static_cast<uint32_t>(std::max(1, sm_count));
        info.available         = true;
        info.is_integrated_gpu = (integrated != 0);

        if (pfn_cuCtxCreate && pfn_cuMemAlloc && pfn_cuMemcpyHtoD && pfn_cuMemcpyDtoH) {
            if (pfn_cuCtxCreate(&cu_ctx, 0, cu_device) == CUDA_SUCCESS_VAL && cu_ctx) {
                cu_ready = true;
            }
        }

        return info;
    }

    bool ExecuteCudaDmaRoundtrip(const void* src_data, void* dst_data, size_t byte_size) noexcept {
        if (!cu_ready || !cu_ctx || byte_size == 0) return false;
        if (pfn_cuCtxSetCurrent) {
            pfn_cuCtxSetCurrent(cu_ctx);
        }
        if (byte_size > cu_dptr_cap) {
            if (cu_dptr && pfn_cuMemFree) {
                pfn_cuMemFree(cu_dptr);
                cu_dptr = 0;
            }
            if (pfn_cuMemAlloc(&cu_dptr, byte_size) != CUDA_SUCCESS_VAL || !cu_dptr) {
                cu_dptr_cap = 0;
                return false;
            }
            cu_dptr_cap = byte_size;
        }
        if (pfn_cuMemcpyHtoD(cu_dptr, src_data, byte_size) != CUDA_SUCCESS_VAL) {
            return false;
        }
        if (dst_data) {
            if (pfn_cuMemcpyDtoH(dst_data, cu_dptr, byte_size) != CUDA_SUCCESS_VAL) {
                return false;
            }
        } else if (pfn_cuCtxSynchronize) {
            pfn_cuCtxSynchronize();
        }
        return true;
    }

    // ------------------------------------------------------------------------
    // 5. AMD ROCm / HIP Runtime API Probing & Runtime Setup
    // ------------------------------------------------------------------------
    ComputeDeviceInfo ProbeRocmBackend() noexcept {
        ShutdownROCm();

        ComputeDeviceInfo info{};
        info.backend = ComputeBackend::ROCM;
        CopyFixedString(info.backend_name, sizeof(info.backend_name), "ROCm");
        CopyFixedString(info.device_name, sizeof(info.device_name), "AMD ROCm / HIP Not Detected");
        CopyFixedString(info.driver_version, sizeof(info.driver_version), "N/A");
        info.available = false;

#if defined(_WIN32)
        const char* const kHipLibs[] = {"amdhip64.dll", "amdhip64_6.dll", "amdhip64_5.dll"};
#else
        const char* const kHipLibs[] = {"libamdhip64.so", "libamdhip64.so.6", "libamdhip64.so.5"};
#endif
        hip_lib = OpenDynamicLibrary(kHipLibs, sizeof(kHipLibs) / sizeof(kHipLibs[0]));
        if (!hip_lib) {
            return info;
        }

        LoadSymbol(hip_lib, "hipInit", pfn_hipInit);
        LoadSymbol(hip_lib, "hipDriverGetVersion", pfn_hipDriverGetVersion);
        LoadSymbol(hip_lib, "hipRuntimeGetVersion", pfn_hipRuntimeGetVersion);
        LoadSymbol(hip_lib, "hipGetDeviceCount", pfn_hipGetDeviceCount);
        LoadSymbol(hip_lib, "hipDeviceGet", pfn_hipDeviceGet);
        LoadSymbol(hip_lib, "hipDeviceGetName", pfn_hipDeviceGetName);
        LoadSymbol(hip_lib, "hipDeviceTotalMem", pfn_hipDeviceTotalMem);
        LoadSymbol(hip_lib, "hipMemGetInfo", pfn_hipMemGetInfo);
        LoadSymbol(hip_lib, "hipDeviceGetAttribute", pfn_hipDeviceGetAttribute);
        LoadSymbol(hip_lib, "hipMalloc", pfn_hipMalloc);
        LoadSymbol(hip_lib, "hipFree", pfn_hipFree);
        LoadSymbol(hip_lib, "hipMemcpy", pfn_hipMemcpy);
        LoadSymbol(hip_lib, "hipDeviceSynchronize", pfn_hipDeviceSynchronize);

        if (!pfn_hipGetDeviceCount) {
            ShutdownROCm();
            return info;
        }

        if (pfn_hipInit && pfn_hipInit(0) != HIP_SUCCESS_VAL) {
            ShutdownROCm();
            return info;
        }

        int32_t dev_count = 0;
        if (pfn_hipGetDeviceCount(&dev_count) != HIP_SUCCESS_VAL || dev_count <= 0) {
            ShutdownROCm();
            return info;
        }

        hip_device = 0;
        if (pfn_hipDeviceGet) {
            pfn_hipDeviceGet(&hip_device, 0);
        }

        char dev_name[128] = {0};
        if (pfn_hipDeviceGetName) {
            pfn_hipDeviceGetName(dev_name, sizeof(dev_name) - 1, hip_device);
        }
        TrimAsciiWhitespace(dev_name);
        CopyFixedString(info.device_name, sizeof(info.device_name),
                        dev_name[0] ? dev_name : "AMD Radeon ROCm / HIP GPU");

        size_t total_mem = 0;
        size_t free_mem  = 0;
        if (pfn_hipDeviceTotalMem && pfn_hipDeviceTotalMem(&total_mem, hip_device) == HIP_SUCCESS_VAL && total_mem > 0) {
            info.vram_bytes = static_cast<uint64_t>(total_mem);
        } else if (pfn_hipMemGetInfo && pfn_hipMemGetInfo(&free_mem, &total_mem) == HIP_SUCCESS_VAL) {
            info.vram_bytes = static_cast<uint64_t>(total_mem);
        }

        int32_t cu_count   = 0;
        int32_t integrated = 0;
        if (pfn_hipDeviceGetAttribute) {
            pfn_hipDeviceGetAttribute(&cu_count, HIP_DEVICE_ATTR_MULTIPROCESSOR_COUNT, hip_device);
            pfn_hipDeviceGetAttribute(&integrated, HIP_DEVICE_ATTR_INTEGRATED, hip_device);
        }

        int32_t hip_ver = 0;
        if (pfn_hipRuntimeGetVersion) {
            pfn_hipRuntimeGetVersion(&hip_ver);
        } else if (pfn_hipDriverGetVersion) {
            pfn_hipDriverGetVersion(&hip_ver);
        }

        char ver_buf[64] = {0};
        if (hip_ver > 0) {
            const int maj = hip_ver / 10000000;
            const int min = (hip_ver / 100000) % 100;
            if (maj > 0) {
                std::snprintf(ver_buf, sizeof(ver_buf), "ROCm / HIP %d.%d", maj, min);
            } else {
                std::snprintf(ver_buf, sizeof(ver_buf), "ROCm / HIP (%d)", hip_ver);
            }
        } else {
            std::snprintf(ver_buf, sizeof(ver_buf), "AMD HIP Runtime");
        }
        CopyFixedString(info.driver_version, sizeof(info.driver_version), ver_buf);

        info.compute_units     = static_cast<uint32_t>(std::max(1, cu_count));
        info.available         = true;
        info.is_integrated_gpu = (integrated != 0) || InferIntegratedGpuFromName(dev_name, "AMD");

        if (pfn_hipMalloc && pfn_hipFree && pfn_hipMemcpy) {
            hip_ready = true;
        }

        return info;
    }

    bool ExecuteHipDmaRoundtrip(const void* src_data, void* dst_data, size_t byte_size) noexcept {
        if (!hip_ready || byte_size == 0) return false;
        if (byte_size > hip_dptr_cap) {
            if (hip_dptr && pfn_hipFree) {
                pfn_hipFree(hip_dptr);
                hip_dptr = nullptr;
            }
            if (pfn_hipMalloc(&hip_dptr, byte_size) != HIP_SUCCESS_VAL || !hip_dptr) {
                hip_dptr_cap = 0;
                return false;
            }
            hip_dptr_cap = byte_size;
        }
        if (pfn_hipMemcpy(hip_dptr, src_data, byte_size, HIP_MEMCPY_HOST_TO_DEVICE_VAL) != HIP_SUCCESS_VAL) {
            return false;
        }
        if (dst_data) {
            if (pfn_hipMemcpy(dst_data, hip_dptr, byte_size, HIP_MEMCPY_DEVICE_TO_HOST_VAL) != HIP_SUCCESS_VAL) {
                return false;
            }
        } else if (pfn_hipDeviceSynchronize) {
            pfn_hipDeviceSynchronize();
        }
        return true;
    }

    // ------------------------------------------------------------------------
    // Probe All Backends & Resolve Optimal Active Backend
    // ------------------------------------------------------------------------
    void ProbeAllLocked() {
        devices.clear();
        devices.reserve(5);
        devices.push_back(ProbeCpuBackend());
        devices.push_back(ProbeVulkanBackend());
        devices.push_back(ProbeOpenCLBackend());
        devices.push_back(ProbeCudaBackend());
        devices.push_back(ProbeRocmBackend());
    }

    const ComputeDeviceInfo* FindDeviceLocked(ComputeBackend backend) const noexcept {
        for (const auto& d : devices) {
            if (d.backend == backend) {
                return &d;
            }
        }
        return nullptr;
    }

    ComputeBackend SelectBestAvailableBackendLocked() const noexcept {
        // Priority order for discrete GPUs first: CUDA -> ROCm -> OpenCL -> Vulkan -> CPU
        const ComputeBackend kOrder[] = {
            ComputeBackend::CUDA,
            ComputeBackend::ROCM,
            ComputeBackend::OPENCL,
            ComputeBackend::VULKAN,
            ComputeBackend::CPU
        };
        for (ComputeBackend b : kOrder) {
            const ComputeDeviceInfo* d = FindDeviceLocked(b);
            if (d && d->available && !d->is_integrated_gpu && b != ComputeBackend::CPU) {
                if (b == ComputeBackend::OPENCL && !cl_kernels_ready) continue;
                return b;
            }
        }
        // If on an integrated GPU laptop, OpenCL (with compiled GPU kernels) or Vulkan/CPU SIMD
        if (cl_kernels_ready) {
            const ComputeDeviceInfo* cl_d = FindDeviceLocked(ComputeBackend::OPENCL);
            if (cl_d && cl_d->available) {
                return ComputeBackend::OPENCL;
            }
        }
        if (vk_pipeline_ready) {
            const ComputeDeviceInfo* vk_d = FindDeviceLocked(ComputeBackend::VULKAN);
            if (vk_d && vk_d->available) {
                return ComputeBackend::VULKAN;
            }
        }
        return ComputeBackend::CPU;
    }

    // ------------------------------------------------------------------------
    // OpenCL Kernel Execution Helpers
    // ------------------------------------------------------------------------
    bool BlendOpenCLLocked(const FrameView& foreground,
                           const FrameView& background,
                           const FrameView& alpha_mask,
                           FrameView& output) noexcept {
        if (!cl_kernels_ready) return false;
        const size_t rgba_bytes = foreground.ByteSize();
        const size_t mask_bytes = alpha_mask.ByteSize();
        if (!EnsureOpenCLBuffers(rgba_bytes, mask_bytes, 0)) return false;

        if (pfn_clEnqueueWriteBuffer(cl_queue, cl_buf_fg, CL_TRUE_VAL, 0, rgba_bytes, foreground.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL ||
            pfn_clEnqueueWriteBuffer(cl_queue, cl_buf_bg, CL_TRUE_VAL, 0, rgba_bytes, background.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL ||
            pfn_clEnqueueWriteBuffer(cl_queue, cl_buf_mask, CL_TRUE_VAL, 0, mask_bytes, alpha_mask.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        const cl_uint width         = foreground.width;
        const cl_uint height        = foreground.height;
        const cl_uint rgba_pitch_px = foreground.stride / 4u;
        const cl_uint mask_stride   = alpha_mask.stride;

        pfn_clSetKernelArg(cl_kern_blend, 0, sizeof(cl_mem), &cl_buf_fg);
        pfn_clSetKernelArg(cl_kern_blend, 1, sizeof(cl_mem), &cl_buf_bg);
        pfn_clSetKernelArg(cl_kern_blend, 2, sizeof(cl_mem), &cl_buf_mask);
        pfn_clSetKernelArg(cl_kern_blend, 3, sizeof(cl_mem), &cl_buf_out);
        pfn_clSetKernelArg(cl_kern_blend, 4, sizeof(cl_uint), &width);
        pfn_clSetKernelArg(cl_kern_blend, 5, sizeof(cl_uint), &height);
        pfn_clSetKernelArg(cl_kern_blend, 6, sizeof(cl_uint), &rgba_pitch_px);
        pfn_clSetKernelArg(cl_kern_blend, 7, sizeof(cl_uint), &mask_stride);

        const size_t global_work_size[2] = {static_cast<size_t>(width), static_cast<size_t>(height)};
        if (pfn_clEnqueueNDRangeKernel(cl_queue, cl_kern_blend, 2, nullptr, global_work_size, nullptr, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }
        if (pfn_clEnqueueReadBuffer(cl_queue, cl_buf_out, CL_TRUE_VAL, 0, output.ByteSize(), output.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        output.timestamp_ns = foreground.timestamp_ns;
        output.frame_index  = foreground.frame_index;
        return true;
    }

    bool BlurOpenCLLocked(const FrameView& src, FrameView& dst_bg, uint32_t radius) noexcept {
        if (!cl_kernels_ready) return false;
        const size_t rgba_bytes = src.ByteSize();
        if (!EnsureOpenCLBuffers(rgba_bytes, 0, 0)) return false;

        if (pfn_clEnqueueWriteBuffer(cl_queue, cl_buf_fg, CL_TRUE_VAL, 0, rgba_bytes, src.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        const cl_uint width    = src.width;
        const cl_uint height   = src.height;
        const cl_uint pitch_px = src.stride / 4u;
        const cl_int  rad      = static_cast<cl_int>(std::clamp(radius, 1u, 64u));
        const size_t  gws[2]   = {static_cast<size_t>(width), static_cast<size_t>(height)};

        // Horizontal pass: cl_buf_fg -> cl_buf_bg
        pfn_clSetKernelArg(cl_kern_blur_h, 0, sizeof(cl_mem), &cl_buf_fg);
        pfn_clSetKernelArg(cl_kern_blur_h, 1, sizeof(cl_mem), &cl_buf_bg);
        pfn_clSetKernelArg(cl_kern_blur_h, 2, sizeof(cl_uint), &width);
        pfn_clSetKernelArg(cl_kern_blur_h, 3, sizeof(cl_uint), &height);
        pfn_clSetKernelArg(cl_kern_blur_h, 4, sizeof(cl_uint), &pitch_px);
        pfn_clSetKernelArg(cl_kern_blur_h, 5, sizeof(cl_int), &rad);

        if (pfn_clEnqueueNDRangeKernel(cl_queue, cl_kern_blur_h, 2, nullptr, gws, nullptr, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        // Vertical pass: cl_buf_bg -> cl_buf_out
        pfn_clSetKernelArg(cl_kern_blur_v, 0, sizeof(cl_mem), &cl_buf_bg);
        pfn_clSetKernelArg(cl_kern_blur_v, 1, sizeof(cl_mem), &cl_buf_out);
        pfn_clSetKernelArg(cl_kern_blur_v, 2, sizeof(cl_uint), &width);
        pfn_clSetKernelArg(cl_kern_blur_v, 3, sizeof(cl_uint), &height);
        pfn_clSetKernelArg(cl_kern_blur_v, 4, sizeof(cl_uint), &pitch_px);
        pfn_clSetKernelArg(cl_kern_blur_v, 5, sizeof(cl_int), &rad);

        if (pfn_clEnqueueNDRangeKernel(cl_queue, cl_kern_blur_v, 2, nullptr, gws, nullptr, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        if (pfn_clEnqueueReadBuffer(cl_queue, cl_buf_out, CL_TRUE_VAL, 0, dst_bg.ByteSize(), dst_bg.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        dst_bg.timestamp_ns = src.timestamp_ns;
        dst_bg.frame_index  = src.frame_index;
        return true;
    }

    bool AiUpsampleOpenCLLocked(const float* smoothed_prob,
                                uint32_t map_width,
                                uint32_t map_height,
                                FrameView& out_mask) noexcept {
        if (!cl_kernels_ready) return false;
        const size_t prob_bytes = static_cast<size_t>(map_width) * map_height * sizeof(float);
        const size_t mask_bytes = out_mask.ByteSize();
        if (!EnsureOpenCLBuffers(64, mask_bytes, prob_bytes)) return false;

        if (pfn_clEnqueueWriteBuffer(cl_queue, cl_buf_prob, CL_TRUE_VAL, 0, prob_bytes, smoothed_prob, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }

        const cl_uint mw = map_width;
        const cl_uint mh = map_height;
        const cl_uint ow = out_mask.width;
        const cl_uint oh = out_mask.height;
        const cl_uint os = out_mask.stride;

        pfn_clSetKernelArg(cl_kern_ai_up, 0, sizeof(cl_mem), &cl_buf_prob);
        pfn_clSetKernelArg(cl_kern_ai_up, 1, sizeof(cl_mem), &cl_buf_mask);
        pfn_clSetKernelArg(cl_kern_ai_up, 2, sizeof(cl_uint), &mw);
        pfn_clSetKernelArg(cl_kern_ai_up, 3, sizeof(cl_uint), &mh);
        pfn_clSetKernelArg(cl_kern_ai_up, 4, sizeof(cl_uint), &ow);
        pfn_clSetKernelArg(cl_kern_ai_up, 5, sizeof(cl_uint), &oh);
        pfn_clSetKernelArg(cl_kern_ai_up, 6, sizeof(cl_uint), &os);

        const size_t gws[2] = {static_cast<size_t>(ow), static_cast<size_t>(oh)};
        if (pfn_clEnqueueNDRangeKernel(cl_queue, cl_kern_ai_up, 2, nullptr, gws, nullptr, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }
        if (pfn_clEnqueueReadBuffer(cl_queue, cl_buf_mask, CL_TRUE_VAL, 0, mask_bytes, out_mask.data, 0, nullptr, nullptr) != CL_SUCCESS_VAL) {
            return false;
        }
        return true;
    }

    // ------------------------------------------------------------------------
    // CPU Bilinear Upsample + EMA Post-Process
    // ------------------------------------------------------------------------
    bool AiPostProcessCpuLocked(const float* prob_map,
                                uint32_t map_width,
                                uint32_t map_height,
                                FrameView& out_mask,
                                float temporal_smooth) noexcept {
        const size_t count = static_cast<size_t>(map_width) * map_height;
        const float alpha  = std::clamp(temporal_smooth, 0.0f, 0.95f);
        if (ema_prob_cache.size() != count) {
            ema_prob_cache.assign(prob_map, prob_map + count);
        } else if (alpha > 0.001f) {
            const float one_minus = 1.0f - alpha;
            for (size_t i = 0; i < count; ++i) {
                ema_prob_cache[i] = ema_prob_cache[i] * alpha + prob_map[i] * one_minus;
            }
        } else {
            std::memcpy(ema_prob_cache.data(), prob_map, count * sizeof(float));
        }

        const uint32_t out_w = out_mask.width;
        const uint32_t out_h = out_mask.height;
        const float scale_x  = static_cast<float>(map_width) / static_cast<float>(out_w);
        const float scale_y  = static_cast<float>(map_height) / static_cast<float>(out_h);

        for (uint32_t y = 0; y < out_h; ++y) {
            const float gy = (static_cast<float>(y) + 0.5f) * scale_y - 0.5f;
            const int32_t y0 = std::clamp(static_cast<int32_t>(std::floor(gy)), 0, static_cast<int32_t>(map_height) - 1);
            const int32_t y1 = std::min(y0 + 1, static_cast<int32_t>(map_height) - 1);
            const float fy   = std::clamp(gy - std::floor(gy), 0.0f, 1.0f);

            const float* row0 = ema_prob_cache.data() + static_cast<size_t>(y0) * map_width;
            const float* row1 = ema_prob_cache.data() + static_cast<size_t>(y1) * map_width;
            uint8_t* dst_row  = out_mask.data + static_cast<size_t>(y) * out_mask.stride;

            for (uint32_t x = 0; x < out_w; ++x) {
                const float gx = (static_cast<float>(x) + 0.5f) * scale_x - 0.5f;
                const int32_t x0 = std::clamp(static_cast<int32_t>(std::floor(gx)), 0, static_cast<int32_t>(map_width) - 1);
                const int32_t x1 = std::min(x0 + 1, static_cast<int32_t>(map_width) - 1);
                const float fx   = std::clamp(gx - std::floor(gx), 0.0f, 1.0f);

                const float top = row0[x0] + (row0[x1] - row0[x0]) * fx;
                const float bot = row1[x0] + (row1[x1] - row1[x0]) * fx;
                const float val = std::clamp(top + (bot - top) * fy, 0.0f, 1.0f);
                dst_row[x]      = static_cast<uint8_t>(val * 255.0f + 0.5f);
            }
        }
        return true;
    }
};

// ============================================================================
// GpuAccelerator Public Implementation
// ============================================================================

GpuAccelerator::GpuAccelerator(ComputeBackend initial_backend, SimdTier initial_simd)
    : impl_(std::make_unique<RuntimeImpl>(initial_simd)) {
    ProbeAvailableBackends();
    SetActiveBackend(initial_backend);
}

GpuAccelerator::~GpuAccelerator() = default;

std::vector<ComputeDeviceInfo> GpuAccelerator::ProbeAvailableBackends() {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    impl_->ProbeAllLocked();
    if (impl_->requested_backend == ComputeBackend::AUTO) {
        impl_->active_backend = impl_->SelectBestAvailableBackendLocked();
    } else {
        const ComputeDeviceInfo* dev = impl_->FindDeviceLocked(impl_->requested_backend);
        impl_->active_backend = (dev && dev->available) ? impl_->requested_backend : ComputeBackend::CPU;
    }
    return impl_->devices;
}

const std::vector<ComputeDeviceInfo>& GpuAccelerator::GetCachedDevices() const noexcept {
    return impl_->devices;
}

uint32_t GpuAccelerator::GetDeviceCount() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    return static_cast<uint32_t>(impl_->devices.size());
}

bool GpuAccelerator::GetDeviceInfoByIndex(uint32_t index, ComputeDeviceInfo* out_info) const noexcept {
    if (!out_info) return false;
    std::lock_guard<std::mutex> lock(impl_->mutex);
    if (index >= impl_->devices.size()) return false;
    *out_info = impl_->devices[index];
    return true;
}

bool GpuAccelerator::GetDeviceInfoByBackend(ComputeBackend backend, ComputeDeviceInfo* out_info) const noexcept {
    if (!out_info) return false;
    std::lock_guard<std::mutex> lock(impl_->mutex);
    const ComputeBackend target = (backend == ComputeBackend::AUTO) ? impl_->active_backend : backend;
    const ComputeDeviceInfo* d = impl_->FindDeviceLocked(target);
    if (!d) return false;
    *out_info = *d;
    return true;
}

bool GpuAccelerator::IsBackendAvailable(ComputeBackend backend) const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    if (backend == ComputeBackend::AUTO || backend == ComputeBackend::CPU) return true;
    const ComputeDeviceInfo* d = impl_->FindDeviceLocked(backend);
    return d != nullptr && d->available;
}

bool GpuAccelerator::SetActiveBackend(ComputeBackend backend) {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    impl_->requested_backend = backend;
    if (backend == ComputeBackend::AUTO) {
        impl_->active_backend = impl_->SelectBestAvailableBackendLocked();
        return true;
    }
    const ComputeDeviceInfo* d = impl_->FindDeviceLocked(backend);
    if (d && d->available) {
        impl_->active_backend = backend;
        return true;
    }
    impl_->active_backend = ComputeBackend::CPU;
    return false;
}

ComputeBackend GpuAccelerator::GetActiveBackend() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    return impl_->active_backend;
}

ComputeBackend GpuAccelerator::GetRequestedBackend() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    return impl_->requested_backend;
}

const char* GpuAccelerator::GetActiveBackendName() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    return ComputeBackendToString(impl_->active_backend);
}

const char* GpuAccelerator::GetActiveDeviceName() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    const ComputeDeviceInfo* d = impl_->FindDeviceLocked(impl_->active_backend);
    if (d) {
        return d->device_name;
    }
    return "Generic CPU";
}

void GpuAccelerator::SetSimdTier(SimdTier tier) noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    impl_->cpu_compositor.SetSimdTier(tier);
}

SimdTier GpuAccelerator::GetSimdTier() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    return impl_->cpu_compositor.GetSimdTier();
}

const char* GpuAccelerator::GetActiveSimdName() const noexcept {
    std::lock_guard<std::mutex> lock(impl_->mutex);
    return impl_->cpu_compositor.GetActiveSimdBackend();
}

bool GpuAccelerator::AccelerateBlurBgra(const FrameView& src, FrameView& dst_bg, uint32_t radius) {
    if (!src.IsValid() || !dst_bg.IsValid()) return false;

    std::lock_guard<std::mutex> lock(impl_->mutex);
    switch (impl_->active_backend) {
        case ComputeBackend::OPENCL:
            if (impl_->BlurOpenCLLocked(src, dst_bg, radius)) {
                return true;
            }
            break;
        case ComputeBackend::VULKAN:
            SimdCompositor::GenerateBlurredBackground(src, dst_bg, radius);
            impl_->ExecuteVulkanDmaRoundtrip(dst_bg.data, nullptr, dst_bg.ByteSize());
            return true;
        case ComputeBackend::CUDA:
            SimdCompositor::GenerateBlurredBackground(src, dst_bg, radius);
            impl_->ExecuteCudaDmaRoundtrip(dst_bg.data, nullptr, dst_bg.ByteSize());
            return true;
        case ComputeBackend::ROCM:
            SimdCompositor::GenerateBlurredBackground(src, dst_bg, radius);
            impl_->ExecuteHipDmaRoundtrip(dst_bg.data, nullptr, dst_bg.ByteSize());
            return true;
        case ComputeBackend::CPU:
        case ComputeBackend::AUTO:
        default:
            break;
    }
    return SimdCompositor::GenerateBlurredBackground(src, dst_bg, radius);
}

bool GpuAccelerator::AccelerateBlendBgra(const FrameView& foreground,
                                         const FrameView& background,
                                         const FrameView& alpha_mask,
                                         FrameView& output) {
    if (!foreground.IsValid() || !background.IsValid() || !alpha_mask.IsValid() || !output.IsValid()) {
        return false;
    }

    std::lock_guard<std::mutex> lock(impl_->mutex);
    switch (impl_->active_backend) {
        case ComputeBackend::OPENCL:
            if (impl_->BlendOpenCLLocked(foreground, background, alpha_mask, output)) {
                return true;
            }
            break;
        case ComputeBackend::VULKAN:
            if (impl_->cpu_compositor.Blend(foreground, background, alpha_mask, output)) {
                impl_->ExecuteVulkanDmaRoundtrip(output.data, nullptr, output.ByteSize());
                return true;
            }
            break;
        case ComputeBackend::CUDA:
            if (impl_->cpu_compositor.Blend(foreground, background, alpha_mask, output)) {
                impl_->ExecuteCudaDmaRoundtrip(output.data, nullptr, output.ByteSize());
                return true;
            }
            break;
        case ComputeBackend::ROCM:
            if (impl_->cpu_compositor.Blend(foreground, background, alpha_mask, output)) {
                impl_->ExecuteHipDmaRoundtrip(output.data, nullptr, output.ByteSize());
                return true;
            }
            break;
        case ComputeBackend::CPU:
        case ComputeBackend::AUTO:
        default:
            break;
    }
    return impl_->cpu_compositor.Blend(foreground, background, alpha_mask, output);
}

bool GpuAccelerator::AccelerateAiPostProcess(const float* prob_map,
                                             uint32_t map_width,
                                             uint32_t map_height,
                                             FrameView& out_mask,
                                             float temporal_smooth) {
    if (!prob_map || map_width == 0 || map_height == 0 || !out_mask.IsValid() ||
        out_mask.format != PixelFormat::GRAY8) {
        return false;
    }

    std::lock_guard<std::mutex> lock(impl_->mutex);
    if (impl_->active_backend == ComputeBackend::OPENCL && impl_->cl_kernels_ready) {
        const size_t count = static_cast<size_t>(map_width) * map_height;
        const float alpha  = std::clamp(temporal_smooth, 0.0f, 0.95f);
        if (impl_->ema_prob_cache.size() != count) {
            impl_->ema_prob_cache.assign(prob_map, prob_map + count);
        } else if (alpha > 0.001f) {
            const float one_minus = 1.0f - alpha;
            for (size_t i = 0; i < count; ++i) {
                impl_->ema_prob_cache[i] = impl_->ema_prob_cache[i] * alpha + prob_map[i] * one_minus;
            }
        } else {
            std::memcpy(impl_->ema_prob_cache.data(), prob_map, count * sizeof(float));
        }

        if (impl_->AiUpsampleOpenCLLocked(impl_->ema_prob_cache.data(), map_width, map_height, out_mask)) {
            return true;
        }
    }

    return impl_->AiPostProcessCpuLocked(prob_map, map_width, map_height, out_mask, temporal_smooth);
}

std::vector<BackendBenchmarkResult> GpuAccelerator::RunMicroBenchmark(uint32_t width,
                                                                      uint32_t height,
                                                                      uint32_t iterations) {
    const uint32_t bench_w    = std::clamp(width, 320u, 3840u);
    const uint32_t bench_h    = std::clamp(height, 240u, 2160u);
    const uint32_t bench_iter = std::clamp(iterations, 2u, 60u);

    FrameBuffer fg_buf(bench_w, bench_h, PixelFormat::BGRA32);
    FrameBuffer bg_buf(bench_w, bench_h, PixelFormat::BGRA32);
    FrameBuffer mask_buf(bench_w, bench_h, PixelFormat::GRAY8);
    FrameBuffer out_buf(bench_w, bench_h, PixelFormat::BGRA32);

    // Populate synthetic portrait test pattern
    for (uint32_t y = 0; y < bench_h; ++y) {
        uint8_t* fg_row   = fg_buf.Data() + static_cast<size_t>(y) * fg_buf.Stride();
        uint8_t* bg_row   = bg_buf.Data() + static_cast<size_t>(y) * bg_buf.Stride();
        uint8_t* mask_row = mask_buf.Data() + static_cast<size_t>(y) * mask_buf.Stride();
        for (uint32_t x = 0; x < bench_w; ++x) {
            fg_row[x * 4 + 0] = static_cast<uint8_t>((x + y) & 0xFFu);
            fg_row[x * 4 + 1] = static_cast<uint8_t>((x * 2) & 0xFFu);
            fg_row[x * 4 + 2] = static_cast<uint8_t>((y * 2) & 0xFFu);
            fg_row[x * 4 + 3] = 255;

            bg_row[x * 4 + 0] = 48;
            bg_row[x * 4 + 1] = 96;
            bg_row[x * 4 + 2] = 32;
            bg_row[x * 4 + 3] = 255;

            mask_row[x] = static_cast<uint8_t>((x * 255u) / bench_w);
        }
    }

    constexpr uint32_t kAiTensorW = 256;
    constexpr uint32_t kAiTensorH = 144;
    std::vector<float> ai_tensor(static_cast<size_t>(kAiTensorW) * kAiTensorH * 3, 0.5f);
    std::vector<float> ai_prob(static_cast<size_t>(kAiTensorW) * kAiTensorH, 0.75f);

    std::lock_guard<std::mutex> lock(impl_->mutex);
    if (impl_->devices.empty()) {
        impl_->ProbeAllLocked();
    }

    const SimdTier max_cpu_simd  = SimdCompositor::DetectBestSimdTier();
    const SimdTier saved_simd    = impl_->cpu_compositor.GetSimdTier();
    const unsigned int hw_cores  = std::max(1u, std::thread::hardware_concurrency());

    struct CandidateConfig {
        ComputeBackend backend;
        uint32_t       simd_mode; // 0=Auto, 1=AVX512, 2=AVX2, 3=Scalar
        SimdTier       tier;
        bool           supported;
        float          gpu_offload_factor;
    };

    const ComputeDeviceInfo* vk_dev   = impl_->FindDeviceLocked(ComputeBackend::VULKAN);
    const ComputeDeviceInfo* cl_dev   = impl_->FindDeviceLocked(ComputeBackend::OPENCL);
    const ComputeDeviceInfo* cuda_dev = impl_->FindDeviceLocked(ComputeBackend::CUDA);
    const ComputeDeviceInfo* rocm_dev = impl_->FindDeviceLocked(ComputeBackend::ROCM);

    const CandidateConfig kCandidates[] = {
        {ComputeBackend::CPU,    1u, SimdTier::AVX512, max_cpu_simd >= SimdTier::AVX512, 0.0f},
        {ComputeBackend::CPU,    2u, SimdTier::AVX2,   max_cpu_simd >= SimdTier::AVX2,   0.0f},
        {ComputeBackend::CPU,    3u, SimdTier::SCALAR, true,                             0.0f},
        {ComputeBackend::VULKAN, 0u, max_cpu_simd,     vk_dev && vk_dev->available,      0.55f},
        {ComputeBackend::OPENCL, 0u, max_cpu_simd,     cl_dev && cl_dev->available,      0.72f},
        {ComputeBackend::CUDA,   0u, max_cpu_simd,     cuda_dev && cuda_dev->available,  0.80f},
        {ComputeBackend::ROCM,   0u, max_cpu_simd,     rocm_dev && rocm_dev->available,  0.78f}
    };

    std::vector<BackendBenchmarkResult> results;
    results.reserve((sizeof(kCandidates) / sizeof(kCandidates[0])) * 2);

    for (const auto& cand : kCandidates) {
        if (!cand.supported) {
            continue;
        }
        for (int ai_pass = 0; ai_pass < 2; ++ai_pass) {
            const bool ai_enabled = (ai_pass == 1);
            BackendBenchmarkResult res{};
            res.backend    = cand.backend;
            res.simd_mode  = cand.simd_mode;
            res.ai_enabled = ai_enabled;
            res.supported  = true;

            impl_->cpu_compositor.SetSimdTier(cand.tier);

            // Warm-up pass
            if (cand.backend == ComputeBackend::OPENCL && impl_->cl_kernels_ready) {
                impl_->BlendOpenCLLocked(fg_buf.View(), bg_buf.View(), mask_buf.View(), out_buf.View());
            } else {
                impl_->cpu_compositor.Blend(fg_buf.View(), bg_buf.View(), mask_buf.View(), out_buf.View());
            }

            const auto t0 = std::chrono::steady_clock::now();
            for (uint32_t it = 0; it < bench_iter; ++it) {
                if (ai_enabled) {
                    AiOnnxSegmenter::PrepareInputTensorNHWC(fg_buf.View(), kAiTensorW, kAiTensorH, ai_tensor.data());
                    if (cand.backend == ComputeBackend::OPENCL && impl_->cl_kernels_ready) {
                        impl_->AiUpsampleOpenCLLocked(ai_prob.data(), kAiTensorW, kAiTensorH, mask_buf.View());
                    } else {
                        impl_->AiPostProcessCpuLocked(ai_prob.data(), kAiTensorW, kAiTensorH, mask_buf.View(), 0.25f);
                    }
                }

                if (cand.backend == ComputeBackend::OPENCL && impl_->cl_kernels_ready) {
                    if ((it & 3u) == 0u) {
                        impl_->BlurOpenCLLocked(fg_buf.View(), bg_buf.View(), 6);
                    }
                    impl_->BlendOpenCLLocked(fg_buf.View(), bg_buf.View(), mask_buf.View(), out_buf.View());
                } else {
                    if ((it & 3u) == 0u) {
                        SimdCompositor::GenerateBlurredBackground(fg_buf.View(), bg_buf.View(), 4);
                    }
                    impl_->cpu_compositor.Blend(fg_buf.View(), bg_buf.View(), mask_buf.View(), out_buf.View());

                    if (cand.backend == ComputeBackend::VULKAN) {
                        impl_->ExecuteVulkanDmaRoundtrip(out_buf.Data(), nullptr, out_buf.SizeBytes());
                    } else if (cand.backend == ComputeBackend::CUDA) {
                        impl_->ExecuteCudaDmaRoundtrip(out_buf.Data(), nullptr, out_buf.SizeBytes());
                    } else if (cand.backend == ComputeBackend::ROCM) {
                        impl_->ExecuteHipDmaRoundtrip(out_buf.Data(), nullptr, out_buf.SizeBytes());
                    }
                }
            }
            const auto t1 = std::chrono::steady_clock::now();

            const double total_ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
            const float avg_ms    = static_cast<float>(std::max(0.01, total_ms / static_cast<double>(bench_iter)));
            const float est_fps   = 1000.0f / avg_ms;

            // Estimate host CPU load at 30 FPS (33.33 ms frame budget)
            const float single_core_duty = (avg_ms / 33.333f) * 100.0f;
            const float host_fraction    = 1.0f - cand.gpu_offload_factor;
            const float raw_cpu_load     = (single_core_duty * host_fraction) / std::max(1.0f, static_cast<float>(hw_cores) * 0.35f);
            const float cpu_load         = std::clamp(raw_cpu_load, 0.5f, 99.9f);

            // Synthetic score: rewards high FPS and low host CPU utilization without hard artificial cap
            const float fps_norm  = est_fps / 120.0f;
            const float eff_norm  = std::clamp((100.0f - cpu_load) / 100.0f, 0.0f, 1.0f);
            const float raw_score = (fps_norm * 6500.0f) + (eff_norm * 3500.0f);

            res.avg_frame_ms               = avg_ms;
            res.estimated_fps              = est_fps;
            res.estimated_cpu_load_percent = cpu_load;
            res.score                      = static_cast<uint32_t>(std::max(1L, std::lround(raw_score)));
            results.push_back(res);
        }
    }

    impl_->cpu_compositor.SetSimdTier(saved_simd);
    return results;
}

uint32_t GpuAccelerator::RunMicroBenchmarkC(uint32_t width,
                                            uint32_t height,
                                            uint32_t iterations,
                                            BackendBenchmarkResult* out_results,
                                            uint32_t max_results) {
    const std::vector<BackendBenchmarkResult> all = RunMicroBenchmark(width, height, iterations);
    if (!out_results || max_results == 0) {
        return static_cast<uint32_t>(all.size());
    }
    const uint32_t count = std::min(static_cast<uint32_t>(all.size()), max_results);
    for (uint32_t i = 0; i < count; ++i) {
        out_results[i] = all[i];
    }
    return count;
}

BackendBenchmarkResult GpuAccelerator::GetRecommendedProfile(uint32_t width,
                                                             uint32_t height,
                                                             bool ai_enabled) {
    const std::vector<BackendBenchmarkResult> all = RunMicroBenchmark(width, height, 6);
    BackendBenchmarkResult best{};
    best.backend    = ComputeBackend::CPU;
    best.simd_mode  = 2;
    best.ai_enabled = ai_enabled;
    best.supported  = true;
    best.score      = 0;

    for (const auto& r : all) {
        if (r.supported && r.ai_enabled == ai_enabled && r.score >= best.score) {
            best = r;
        }
    }
    return best;
}

}  // namespace engine
}  // namespace common
}  // namespace smartcam

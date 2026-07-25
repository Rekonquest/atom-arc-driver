#define _WIN32_WINNT 0x0A00

#include <wsl/winadapter.h>
#include <directx/d3d12.h>
#include <directx/dxcore.h>
#include <wrl/client.h>

#include "atom_arc_compute_dxil.h"

#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <cstdint>
#include <cstdlib>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <string_view>
#include <thread>
#include <vector>

using Microsoft::WRL::ComPtr;

namespace {

struct Options {
    std::uint32_t vendor_id = 0x8086;
    std::uint32_t device_id = 0x56a5;
    std::uint32_t timeout_ms = 5000;
    std::uint32_t compute_elements = 1024;
    std::uint32_t compute_addend = 0x13579bdfU;
    bool transfer_lab = false;
    bool compound_lab = false;
    std::uint32_t payload_bytes = 4096;
    std::uint32_t operations_per_sample = 16;
    std::uint32_t samples = 12;
    std::uint32_t warmups = 3;
};

struct Result {
    bool passed = false;
    std::string stage = "startup";
    std::string detail;
    std::string adapter_name;
    std::uint32_t vendor_id = 0;
    std::uint32_t device_id = 0;
    std::uint32_t adapters_seen = 0;
    std::uint32_t hresult = 0;
    bool device_created = false;
    bool queue_created = false;
    bool command_list_closed = false;
    bool resources_created = false;
    bool copy_recorded = false;
    bool submitted = false;
    bool fence_completed = false;
    bool copy_verified = false;
    std::uint64_t bytes_copied = 0;
    std::uint64_t copy_checksum = 0;
    bool compute_pipeline_created = false;
    bool compute_input_uploaded = false;
    bool compute_dispatched = false;
    bool compute_verified = false;
    std::uint32_t compute_elements = 0;
    std::uint32_t compute_addend = 0;
    std::uint64_t compute_input_bytes = 0;
    std::uint64_t compute_output_bytes = 0;
    std::uint64_t compute_checksum = 0;
    std::uint32_t device_removed_reason = 0;
    std::uint64_t elapsed_microseconds = 0;
    std::string mode = "submission-smoke";
    std::string primitive;
    std::uint64_t payload_bytes = 0;
    std::uint32_t operations_per_sample = 0;
    std::uint32_t samples = 0;
    std::uint32_t warmups = 0;
    std::uint64_t pool_setup_ns = 0;
    std::uint64_t candidate_setup_ns = 0;
    std::vector<std::uint64_t> baseline_durations_ns;
    std::vector<std::uint64_t> candidate_durations_ns;
    std::vector<std::uint64_t> correctness_digests;
};

std::string json_escape(std::string_view input) {
    std::ostringstream output;
    for (const unsigned char character : input) {
        switch (character) {
        case '"':
            output << "\\\"";
            break;
        case '\\':
            output << "\\\\";
            break;
        case '\b':
            output << "\\b";
            break;
        case '\f':
            output << "\\f";
            break;
        case '\n':
            output << "\\n";
            break;
        case '\r':
            output << "\\r";
            break;
        case '\t':
            output << "\\t";
            break;
        default:
            if (character < 0x20) {
                output << "\\u" << std::hex << std::setw(4) << std::setfill('0')
                       << static_cast<unsigned int>(character) << std::dec;
            } else {
                output << character;
            }
        }
    }
    return output.str();
}

void print_result(const Result &result) {
    std::cout << "{"
              << "\"schema\":1,"
              << "\"passed\":" << (result.passed ? "true" : "false") << ','
              << "\"backend\":\"wsl-dxcore-d3d12\","
              << "\"stage\":\"" << json_escape(result.stage) << "\","
              << "\"detail\":\"" << json_escape(result.detail) << "\","
              << "\"adapter_name\":\"" << json_escape(result.adapter_name) << "\","
              << "\"vendor_id\":" << result.vendor_id << ','
              << "\"device_id\":" << result.device_id << ','
              << "\"adapters_seen\":" << result.adapters_seen << ','
              << "\"hresult\":" << result.hresult << ','
              << "\"device_created\":" << (result.device_created ? "true" : "false")
              << ','
              << "\"queue_created\":" << (result.queue_created ? "true" : "false") << ','
              << "\"command_list_closed\":"
              << (result.command_list_closed ? "true" : "false") << ','
              << "\"resources_created\":" << (result.resources_created ? "true" : "false")
              << ','
              << "\"copy_recorded\":" << (result.copy_recorded ? "true" : "false") << ','
              << "\"submitted\":" << (result.submitted ? "true" : "false") << ','
              << "\"fence_completed\":" << (result.fence_completed ? "true" : "false")
              << ','
              << "\"copy_verified\":" << (result.copy_verified ? "true" : "false") << ','
              << "\"bytes_copied\":" << result.bytes_copied << ','
              << "\"copy_checksum\":" << result.copy_checksum << ','
              << "\"compute_pipeline_created\":"
              << (result.compute_pipeline_created ? "true" : "false") << ','
              << "\"compute_input_uploaded\":"
              << (result.compute_input_uploaded ? "true" : "false") << ','
              << "\"compute_dispatched\":" << (result.compute_dispatched ? "true" : "false")
              << ','
              << "\"compute_verified\":" << (result.compute_verified ? "true" : "false")
              << ','
              << "\"compute_elements\":" << result.compute_elements << ','
              << "\"compute_addend\":" << result.compute_addend << ','
              << "\"compute_input_bytes\":" << result.compute_input_bytes << ','
              << "\"compute_output_bytes\":" << result.compute_output_bytes << ','
              << "\"compute_checksum\":" << result.compute_checksum << ','
              << "\"device_removed_reason\":" << result.device_removed_reason << ','
              << "\"elapsed_microseconds\":" << result.elapsed_microseconds << ','
              << "\"mode\":\"" << json_escape(result.mode) << "\","
              << "\"primitive\":\"" << json_escape(result.primitive) << "\","
              << "\"payload_bytes\":" << result.payload_bytes << ','
              << "\"operations_per_sample\":" << result.operations_per_sample << ','
              << "\"samples\":" << result.samples << ','
              << "\"warmups\":" << result.warmups << ','
              << "\"pool_setup_ns\":" << result.pool_setup_ns << ','
              << "\"candidate_setup_ns\":" << result.candidate_setup_ns << ',';
    const auto print_values = [](const std::vector<std::uint64_t> &values) {
        std::cout << '[';
        for (std::size_t index = 0; index < values.size(); ++index) {
            if (index != 0) {
                std::cout << ',';
            }
            std::cout << values[index];
        }
        std::cout << ']';
    };
    std::cout << "\"baseline_durations_ns\":";
    print_values(result.baseline_durations_ns);
    std::cout << ",\"candidate_durations_ns\":";
    print_values(result.candidate_durations_ns);
    std::cout << ",\"correctness_digests\":";
    print_values(result.correctness_digests);
    std::cout << "}\n";
}

bool parse_u32(std::string_view value, std::uint32_t &output) {
    std::string storage(value);
    char *end = nullptr;
    const unsigned long parsed = std::strtoul(storage.c_str(), &end, 0);
    if (end == storage.c_str() || *end != '\0' || parsed > UINT32_MAX) {
        return false;
    }
    output = static_cast<std::uint32_t>(parsed);
    return true;
}

bool parse_options(int argc, char **argv, Options &options, std::string &error) {
    for (int index = 1; index < argc; ++index) {
        const std::string_view argument(argv[index]);
        if (argument == "--transfer-lab") {
            options.transfer_lab = true;
            continue;
        }
        if (argument == "--compound-lab") {
            options.compound_lab = true;
            continue;
        }
        if (argument == "--vendor" || argument == "--device" ||
            argument == "--timeout-ms" || argument == "--elements" ||
            argument == "--addend" || argument == "--payload-bytes" ||
            argument == "--operations" || argument == "--samples" ||
            argument == "--warmups") {
            if (index + 1 >= argc) {
                error = std::string(argument) + " requires a value";
                return false;
            }
            std::uint32_t parsed = 0;
            if (!parse_u32(argv[++index], parsed)) {
                error = "invalid numeric value for " + std::string(argument);
                return false;
            }
            if (argument == "--vendor") {
                options.vendor_id = parsed;
            } else if (argument == "--device") {
                options.device_id = parsed;
            } else if (argument == "--timeout-ms") {
                options.timeout_ms = parsed;
            } else if (argument == "--elements") {
                options.compute_elements = parsed;
            } else if (argument == "--addend") {
                options.compute_addend = parsed;
            } else if (argument == "--payload-bytes") {
                options.payload_bytes = parsed;
            } else if (argument == "--operations") {
                options.operations_per_sample = parsed;
            } else if (argument == "--samples") {
                options.samples = parsed;
            } else {
                options.warmups = parsed;
            }
        } else {
            error = "unknown argument: " + std::string(argument);
            return false;
        }
    }
    if (options.timeout_ms == 0 || options.timeout_ms > 60000) {
        error = "timeout must be between 1 and 60000 milliseconds";
        return false;
    }
    if (options.compute_elements == 0 || options.compute_elements > 1024U * 1024U) {
        error = "elements must be between 1 and 1048576";
        return false;
    }
    if (options.transfer_lab && options.compound_lab) {
        error = "transfer and compound labs are mutually exclusive";
        return false;
    }
    const bool any_transfer_lab = options.transfer_lab || options.compound_lab;
    if (any_transfer_lab &&
        (options.payload_bytes < 4096U || options.payload_bytes > 256U * 1024U * 1024U)) {
        error = "payload bytes must be between 4096 and 268435456";
        return false;
    }
    if (any_transfer_lab &&
        (options.operations_per_sample == 0 || options.operations_per_sample > 1024U)) {
        error = "operations must be between 1 and 1024";
        return false;
    }
    if (options.compound_lab && options.operations_per_sample < 2U) {
        error = "compound operations must be between 2 and 1024";
        return false;
    }
    if (any_transfer_lab && (options.samples < 10 || options.samples > 100U)) {
        error = "samples must be between 10 and 100";
        return false;
    }
    if (any_transfer_lab && options.warmups > 20U) {
        error = "warmups must be between 0 and 20";
        return false;
    }
    return true;
}

void fail(Result &result, std::string_view stage, HRESULT hresult, std::string_view detail) {
    result.stage = stage;
    result.hresult = static_cast<std::uint32_t>(hresult);
    result.detail = detail;
}

std::string adapter_description(IDXCoreAdapter *adapter) {
    if (!adapter->IsPropertySupported(DXCoreAdapterProperty::DriverDescription)) {
        return "unknown";
    }
    size_t size = 0;
    if (FAILED(adapter->GetPropertySize(DXCoreAdapterProperty::DriverDescription, &size)) ||
        size == 0 || size > 64 * 1024) {
        return "unknown";
    }
    std::vector<char> value(size + 1, '\0');
    if (FAILED(adapter->GetProperty(DXCoreAdapterProperty::DriverDescription, size,
                                    value.data()))) {
        return "unknown";
    }
    return std::string(value.data());
}

struct TransferBuffers {
    ComPtr<ID3D12Resource> upload;
    ComPtr<ID3D12Resource> device_local;
    ComPtr<ID3D12Resource> readback;
    void *upload_mapping = nullptr;
    void *readback_mapping = nullptr;
};

struct TransferCommands {
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    ComPtr<ID3D12Fence> fence;
    std::uint64_t fence_value = 0;
};

bool create_transfer_commands(ID3D12Device *device, TransferCommands &commands,
                              HRESULT &hresult, std::string &error) {
    hresult = device->CreateCommandAllocator(
        D3D12_COMMAND_LIST_TYPE_COMPUTE, IID_ID3D12CommandAllocator,
        reinterpret_cast<void **>(commands.allocator.GetAddressOf()));
    if (FAILED(hresult)) {
        error = "transfer command allocator creation failed";
        return false;
    }
    hresult = device->CreateCommandList(
        0, D3D12_COMMAND_LIST_TYPE_COMPUTE, commands.allocator.Get(), nullptr,
        IID_ID3D12GraphicsCommandList,
        reinterpret_cast<void **>(commands.list.GetAddressOf()));
    if (FAILED(hresult)) {
        error = "transfer command list creation failed";
        return false;
    }
    hresult = commands.list->Close();
    if (FAILED(hresult)) {
        error = "initial transfer command list close failed";
        return false;
    }
    hresult = device->CreateFence(
        0, D3D12_FENCE_FLAG_NONE, IID_ID3D12Fence,
        reinterpret_cast<void **>(commands.fence.GetAddressOf()));
    if (FAILED(hresult)) {
        error = "transfer fence creation failed";
        return false;
    }
    return true;
}

bool create_transfer_buffers(ID3D12Device *device, std::uint64_t payload_bytes,
                             bool persistent_mapping, TransferBuffers &buffers,
                             HRESULT &hresult, std::string &error) {
    D3D12_RESOURCE_DESC description{};
    description.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    description.Width = payload_bytes;
    description.Height = 1;
    description.DepthOrArraySize = 1;
    description.MipLevels = 1;
    description.Format = DXGI_FORMAT_UNKNOWN;
    description.SampleDesc.Count = 1;
    description.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;

    D3D12_HEAP_PROPERTIES upload_heap{};
    upload_heap.Type = D3D12_HEAP_TYPE_UPLOAD;
    upload_heap.CreationNodeMask = 1;
    upload_heap.VisibleNodeMask = 1;
    hresult = device->CreateCommittedResource(
        &upload_heap, D3D12_HEAP_FLAG_NONE, &description,
        D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(buffers.upload.GetAddressOf()));
    if (FAILED(hresult)) {
        error = "upload staging resource creation failed";
        return false;
    }

    D3D12_HEAP_PROPERTIES device_heap = upload_heap;
    device_heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    hresult = device->CreateCommittedResource(
        &device_heap, D3D12_HEAP_FLAG_NONE, &description,
        D3D12_RESOURCE_STATE_COPY_DEST, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(buffers.device_local.GetAddressOf()));
    if (FAILED(hresult)) {
        error = "device-local transfer resource creation failed";
        return false;
    }

    D3D12_HEAP_PROPERTIES readback_heap = upload_heap;
    readback_heap.Type = D3D12_HEAP_TYPE_READBACK;
    hresult = device->CreateCommittedResource(
        &readback_heap, D3D12_HEAP_FLAG_NONE, &description,
        D3D12_RESOURCE_STATE_COPY_DEST, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(buffers.readback.GetAddressOf()));
    if (FAILED(hresult)) {
        error = "readback staging resource creation failed";
        return false;
    }

    if (!persistent_mapping) {
        return true;
    }
    const D3D12_RANGE no_cpu_read{0, 0};
    hresult = buffers.upload->Map(0, &no_cpu_read, &buffers.upload_mapping);
    if (FAILED(hresult) || buffers.upload_mapping == nullptr) {
        error = "persistent upload mapping failed";
        return false;
    }
    const D3D12_RANGE read_range{0, static_cast<SIZE_T>(payload_bytes)};
    hresult = buffers.readback->Map(0, &read_range, &buffers.readback_mapping);
    if (FAILED(hresult) || buffers.readback_mapping == nullptr) {
        buffers.upload->Unmap(0, &no_cpu_read);
        buffers.upload_mapping = nullptr;
        error = "persistent readback mapping failed";
        return false;
    }
    return true;
}

void release_persistent_mappings(TransferBuffers &buffers, std::uint64_t payload_bytes) {
    if (buffers.upload_mapping != nullptr) {
        const D3D12_RANGE written{0, static_cast<SIZE_T>(payload_bytes)};
        buffers.upload->Unmap(0, &written);
        buffers.upload_mapping = nullptr;
    }
    if (buffers.readback_mapping != nullptr) {
        const D3D12_RANGE no_cpu_write{0, 0};
        buffers.readback->Unmap(0, &no_cpu_write);
        buffers.readback_mapping = nullptr;
    }
}

std::uint8_t transfer_pattern(std::uint64_t byte_index, std::uint32_t sample,
                              std::uint32_t operation) {
    return static_cast<std::uint8_t>(
        ((byte_index * 37U) ^ ((static_cast<std::uint64_t>(sample) + 1U) * 17U) ^
         ((static_cast<std::uint64_t>(operation) + 1U) * 29U)) &
        0xffU);
}

void digest_byte(std::uint64_t &digest, std::uint8_t value) {
    constexpr std::uint64_t fnv_prime = 1099511628211ULL;
    digest ^= value;
    digest *= fnv_prime;
}

void fill_transfer_pattern(void *mapping, std::uint64_t payload_bytes,
                           std::uint32_t sample, std::uint32_t operation,
                           std::uint64_t &digest) {
    auto *bytes = static_cast<std::uint8_t *>(mapping);
    for (std::uint64_t index = 0; index < payload_bytes; ++index) {
        const std::uint8_t value = transfer_pattern(index, sample, operation);
        bytes[index] = value;
        digest_byte(digest, value);
    }
}

bool verify_transfer_pattern(const void *mapping, std::uint64_t payload_bytes,
                             std::uint32_t sample, std::uint32_t operation,
                             std::uint64_t &digest) {
    const auto *bytes = static_cast<const std::uint8_t *>(mapping);
    bool matches = true;
    for (std::uint64_t index = 0; index < payload_bytes; ++index) {
        const std::uint8_t expected = transfer_pattern(index, sample, operation);
        const std::uint8_t actual = bytes[index];
        digest_byte(digest, actual);
        matches = matches && actual == expected;
    }
    return matches;
}

bool submit_transfer_async(ID3D12CommandQueue *queue, TransferCommands &commands,
                           TransferBuffers &buffers, std::uint64_t payload_bytes,
                           Result &result, HRESULT &hresult, std::string &error) {
    hresult = commands.allocator->Reset();
    if (FAILED(hresult)) {
        error = "transfer command allocator reset failed";
        return false;
    }
    hresult = commands.list->Reset(commands.allocator.Get(), nullptr);
    if (FAILED(hresult)) {
        error = "transfer command list reset failed";
        return false;
    }

    commands.list->CopyBufferRegion(buffers.device_local.Get(), 0,
                                    buffers.upload.Get(), 0, payload_bytes);
    D3D12_RESOURCE_BARRIER to_source{};
    to_source.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    to_source.Transition.pResource = buffers.device_local.Get();
    to_source.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    to_source.Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST;
    to_source.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    commands.list->ResourceBarrier(1, &to_source);
    commands.list->CopyBufferRegion(buffers.readback.Get(), 0,
                                    buffers.device_local.Get(), 0, payload_bytes);
    D3D12_RESOURCE_BARRIER to_destination = to_source;
    to_destination.Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_SOURCE;
    to_destination.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_DEST;
    commands.list->ResourceBarrier(1, &to_destination);
    result.copy_recorded = true;

    hresult = commands.list->Close();
    if (FAILED(hresult)) {
        error = "transfer command list close failed";
        return false;
    }
    ID3D12CommandList *lists[] = {commands.list.Get()};
    queue->ExecuteCommandLists(1, lists);
    result.submitted = true;
    ++commands.fence_value;
    hresult = queue->Signal(commands.fence.Get(), commands.fence_value);
    if (FAILED(hresult)) {
        error = "transfer fence signal failed";
        return false;
    }
    return true;
}

bool wait_transfer(ID3D12Device *device, TransferCommands &commands,
                   std::uint32_t timeout_ms, Result &result, HRESULT &hresult,
                   std::string &error) {
    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::milliseconds(timeout_ms);
    while (commands.fence->GetCompletedValue() < commands.fence_value) {
        const HRESULT removed = device->GetDeviceRemovedReason();
        if (FAILED(removed)) {
            hresult = removed;
            result.device_removed_reason = static_cast<std::uint32_t>(removed);
            error = "device removed during transfer benchmark";
            return false;
        }
        if (std::chrono::steady_clock::now() >= deadline) {
            error = "transfer benchmark fence timed out";
            return false;
        }
        std::this_thread::yield();
    }
    result.fence_completed = true;
    return true;
}

bool submit_transfer(ID3D12Device *device, ID3D12CommandQueue *queue,
                     TransferCommands &commands, TransferBuffers &buffers,
                     std::uint64_t payload_bytes, std::uint32_t timeout_ms,
                     Result &result, HRESULT &hresult, std::string &error) {
    return submit_transfer_async(queue, commands, buffers, payload_bytes, result,
                                 hresult, error) &&
           wait_transfer(device, commands, timeout_ms, result, hresult, error);
}

bool run_transfer_batch(ID3D12Device *device, ID3D12CommandQueue *queue,
                        TransferCommands &commands, const Options &options,
                        std::uint32_t sample, TransferBuffers *pooled_buffers,
                        Result &result, std::uint64_t &duration_ns,
                        std::uint64_t &correctness_digest, HRESULT &hresult,
                        std::string &error) {
    constexpr std::uint64_t fnv_offset = 1469598103934665603ULL;
    std::uint64_t expected_digest = fnv_offset;
    std::uint64_t actual_digest = fnv_offset;
    const auto started = std::chrono::steady_clock::now();

    for (std::uint32_t operation = 0; operation < options.operations_per_sample;
         ++operation) {
        TransferBuffers fresh_buffers;
        TransferBuffers *buffers = pooled_buffers;
        if (buffers == nullptr) {
            if (!create_transfer_buffers(device, options.payload_bytes, false,
                                         fresh_buffers, hresult, error)) {
                return false;
            }
            buffers = &fresh_buffers;
        }

        void *upload_mapping = buffers->upload_mapping;
        const D3D12_RANGE no_cpu_read{0, 0};
        if (upload_mapping == nullptr) {
            hresult = buffers->upload->Map(0, &no_cpu_read, &upload_mapping);
            if (FAILED(hresult) || upload_mapping == nullptr) {
                error = "per-transfer upload mapping failed";
                return false;
            }
        }
        fill_transfer_pattern(upload_mapping, options.payload_bytes, sample,
                              operation, expected_digest);
        if (buffers->upload_mapping == nullptr) {
            const D3D12_RANGE written{0, static_cast<SIZE_T>(options.payload_bytes)};
            buffers->upload->Unmap(0, &written);
        }

        if (!submit_transfer(device, queue, commands, *buffers,
                             options.payload_bytes, options.timeout_ms, result,
                             hresult, error)) {
            return false;
        }

        void *readback_mapping = buffers->readback_mapping;
        if (readback_mapping == nullptr) {
            const D3D12_RANGE read_range{
                0, static_cast<SIZE_T>(options.payload_bytes)};
            hresult = buffers->readback->Map(0, &read_range, &readback_mapping);
            if (FAILED(hresult) || readback_mapping == nullptr) {
                error = "per-transfer readback mapping failed";
                return false;
            }
        }
        const bool matches = verify_transfer_pattern(
            readback_mapping, options.payload_bytes, sample, operation,
            actual_digest);
        if (buffers->readback_mapping == nullptr) {
            const D3D12_RANGE no_cpu_write{0, 0};
            buffers->readback->Unmap(0, &no_cpu_write);
        }
        if (!matches) {
            error = "transfer readback did not match the uploaded payload";
            return false;
        }
    }

    duration_ns = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now() - started)
            .count());
    if (actual_digest != expected_digest) {
        error = "transfer digest differed despite byte comparison";
        return false;
    }
    correctness_digest = actual_digest;
    return true;
}

bool run_double_buffer_batch(
    ID3D12Device *device, ID3D12CommandQueue *queue,
    std::array<TransferCommands, 2> &commands,
    std::array<TransferBuffers, 2> &buffers, const Options &options,
    std::uint32_t sample, Result &result,
    std::uint64_t &duration_ns, std::uint64_t &correctness_digest,
    HRESULT &hresult, std::string &error) {
    constexpr std::uint64_t fnv_offset = 1469598103934665603ULL;
    constexpr std::uint32_t slot_count = 2;
    std::uint64_t expected_digest = fnv_offset;
    std::uint64_t actual_digest = fnv_offset;
    const auto started = std::chrono::steady_clock::now();

    const auto wait_and_verify = [&](std::uint32_t operation) {
        const std::size_t slot = operation % buffers.size();
        if (!wait_transfer(device, commands[slot], options.timeout_ms, result,
                           hresult, error)) {
            return false;
        }
        if (!verify_transfer_pattern(buffers[slot].readback_mapping,
                                     options.payload_bytes, sample, operation,
                                     actual_digest)) {
            error = "double-buffered readback did not match the uploaded payload";
            return false;
        }
        return true;
    };

    for (std::uint32_t operation = 0; operation < options.operations_per_sample;
         ++operation) {
        const std::size_t slot = operation % buffers.size();
        if (operation >= slot_count &&
            !wait_and_verify(operation - slot_count)) {
            return false;
        }

        fill_transfer_pattern(buffers[slot].upload_mapping, options.payload_bytes,
                              sample, operation, expected_digest);
        if (!submit_transfer_async(queue, commands[slot], buffers[slot],
                                   options.payload_bytes, result, hresult, error)) {
            return false;
        }
    }

    const std::uint32_t first_pending =
        options.operations_per_sample - slot_count;
    for (std::uint32_t operation = first_pending;
         operation < options.operations_per_sample; ++operation) {
        if (!wait_and_verify(operation)) {
            return false;
        }
    }

    duration_ns = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now() - started)
            .count());
    if (actual_digest != expected_digest) {
        error = "double-buffered digest differed despite byte comparison";
        return false;
    }
    correctness_digest = actual_digest;
    return true;
}

int run_transfer_lab(ID3D12Device *device, ID3D12CommandQueue *queue,
                     const Options &options, Result &result) {
    const auto started = std::chrono::steady_clock::now();
    result.mode = "paired-transfer-lab";
    result.primitive = "reusable-staging-buffer-pool";
    result.payload_bytes = options.payload_bytes;
    result.operations_per_sample = options.operations_per_sample;
    result.samples = options.samples;
    result.warmups = options.warmups;

    HRESULT hr = S_OK;
    TransferCommands commands;
    std::string error;
    if (!create_transfer_commands(device, commands, hr, error)) {
        fail(result, "transfer-create-commands", hr, error);
        return 1;
    }

    TransferBuffers pool;
    const auto pool_started = std::chrono::steady_clock::now();
    if (!create_transfer_buffers(device, options.payload_bytes, true, pool, hr,
                                 error)) {
        fail(result, "transfer-create-pool", hr, error);
        return 1;
    }
    result.pool_setup_ns = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now() - pool_started)
            .count());
    result.resources_created = true;

    auto finish_failure = [&](std::string_view stage) {
        release_persistent_mappings(pool, options.payload_bytes);
        fail(result, stage, hr, error);
        return 1;
    };

    for (std::uint32_t warmup = 0; warmup < options.warmups; ++warmup) {
        std::uint64_t ignored_duration = 0;
        std::uint64_t baseline_digest = 0;
        std::uint64_t candidate_digest = 0;
        if (!run_transfer_batch(device, queue, commands, options,
                                0x80000000U + warmup, nullptr, result,
                                ignored_duration, baseline_digest, hr, error) ||
            !run_transfer_batch(device, queue, commands, options,
                                0x80000000U + warmup, &pool, result,
                                ignored_duration, candidate_digest, hr, error)) {
            return finish_failure("transfer-warmup");
        }
        if (baseline_digest != candidate_digest) {
            error = "baseline and pooled warmup digests differed";
            return finish_failure("transfer-warmup-digest");
        }
    }

    result.baseline_durations_ns.reserve(options.samples);
    result.candidate_durations_ns.reserve(options.samples);
    result.correctness_digests.reserve(options.samples);
    for (std::uint32_t sample = 0; sample < options.samples; ++sample) {
        std::uint64_t baseline_duration = 0;
        std::uint64_t candidate_duration = 0;
        std::uint64_t baseline_digest = 0;
        std::uint64_t candidate_digest = 0;
        const auto baseline = [&]() {
            return run_transfer_batch(device, queue, commands, options, sample,
                                      nullptr, result, baseline_duration,
                                      baseline_digest, hr, error);
        };
        const auto candidate = [&]() {
            return run_transfer_batch(device, queue, commands, options, sample,
                                      &pool, result, candidate_duration,
                                      candidate_digest, hr, error);
        };
        const bool measured = sample % 2U == 0U
                                  ? baseline() && candidate()
                                  : candidate() && baseline();
        if (!measured) {
            return finish_failure("transfer-measurement");
        }
        if (baseline_digest != candidate_digest) {
            error = "baseline and pooled measurement digests differed";
            return finish_failure("transfer-paired-digest");
        }
        result.baseline_durations_ns.push_back(baseline_duration);
        result.candidate_durations_ns.push_back(candidate_duration);
        result.correctness_digests.push_back(baseline_digest);
    }

    release_persistent_mappings(pool, options.payload_bytes);
    result.copy_verified = true;
    result.bytes_copied = static_cast<std::uint64_t>(options.payload_bytes) *
                          options.operations_per_sample * options.samples * 2U;
    result.device_removed_reason =
        static_cast<std::uint32_t>(device->GetDeviceRemovedReason());
    if (FAILED(static_cast<HRESULT>(result.device_removed_reason))) {
        fail(result, "transfer-device-health",
             static_cast<HRESULT>(result.device_removed_reason),
             "device reported removal after transfer measurements");
        return 1;
    }
    result.elapsed_microseconds = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::microseconds>(
            std::chrono::steady_clock::now() - started)
            .count());
    result.stage = "complete";
    result.detail = "paired fresh-versus-reusable staging transfers completed with exact readback on the physical Arc A380 through WSL GPU-PV";
    result.passed = true;
    return 0;
}

int run_compound_lab(ID3D12Device *device, ID3D12CommandQueue *queue,
                     const Options &options, Result &result) {
    const auto started = std::chrono::steady_clock::now();
    result.mode = "paired-compound-transfer-lab";
    result.primitive = "double-buffered-preload";
    result.payload_bytes = options.payload_bytes;
    result.operations_per_sample = options.operations_per_sample;
    result.samples = options.samples;
    result.warmups = options.warmups;

    HRESULT hr = S_OK;
    std::string error;
    TransferCommands baseline_commands;
    if (!create_transfer_commands(device, baseline_commands, hr, error)) {
        fail(result, "compound-create-baseline-commands", hr, error);
        return 1;
    }

    TransferBuffers baseline_pool;
    const auto pool_started = std::chrono::steady_clock::now();
    if (!create_transfer_buffers(device, options.payload_bytes, true,
                                 baseline_pool, hr, error)) {
        fail(result, "compound-create-baseline-pool", hr, error);
        return 1;
    }
    result.pool_setup_ns = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now() - pool_started)
            .count());

    std::array<TransferCommands, 2> candidate_commands;
    std::array<TransferBuffers, 2> candidate_buffers;
    const auto candidate_started = std::chrono::steady_clock::now();
    for (std::size_t slot = 0; slot < candidate_buffers.size(); ++slot) {
        if (!create_transfer_commands(device, candidate_commands[slot], hr,
                                      error) ||
            !create_transfer_buffers(device, options.payload_bytes, true,
                                     candidate_buffers[slot], hr, error)) {
            for (auto &buffers : candidate_buffers) {
                release_persistent_mappings(buffers, options.payload_bytes);
            }
            release_persistent_mappings(baseline_pool, options.payload_bytes);
            fail(result, "compound-create-candidate", hr, error);
            return 1;
        }
    }
    result.candidate_setup_ns = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now() - candidate_started)
            .count());
    result.resources_created = true;

    const auto release_all = [&]() {
        release_persistent_mappings(baseline_pool, options.payload_bytes);
        for (auto &buffers : candidate_buffers) {
            release_persistent_mappings(buffers, options.payload_bytes);
        }
    };
    const auto finish_failure = [&](std::string_view stage) {
        release_all();
        fail(result, stage, hr, error);
        return 1;
    };

    for (std::uint32_t warmup = 0; warmup < options.warmups; ++warmup) {
        std::uint64_t ignored_duration = 0;
        std::uint64_t baseline_digest = 0;
        std::uint64_t candidate_digest = 0;
        if (!run_transfer_batch(device, queue, baseline_commands, options,
                                0x90000000U + warmup, &baseline_pool, result,
                                ignored_duration, baseline_digest, hr, error) ||
            !run_double_buffer_batch(device, queue, candidate_commands,
                                     candidate_buffers, options,
                                     0x90000000U + warmup, result,
                                     ignored_duration, candidate_digest, hr,
                                     error)) {
            return finish_failure("compound-warmup");
        }
        if (baseline_digest != candidate_digest) {
            error = "pooled baseline and double-buffered warmup digests differed";
            return finish_failure("compound-warmup-digest");
        }
    }

    result.baseline_durations_ns.reserve(options.samples);
    result.candidate_durations_ns.reserve(options.samples);
    result.correctness_digests.reserve(options.samples);
    for (std::uint32_t sample = 0; sample < options.samples; ++sample) {
        std::uint64_t baseline_duration = 0;
        std::uint64_t candidate_duration = 0;
        std::uint64_t baseline_digest = 0;
        std::uint64_t candidate_digest = 0;
        const auto baseline = [&]() {
            return run_transfer_batch(device, queue, baseline_commands, options,
                                      sample, &baseline_pool, result,
                                      baseline_duration, baseline_digest, hr,
                                      error);
        };
        const auto candidate = [&]() {
            return run_double_buffer_batch(
                device, queue, candidate_commands, candidate_buffers, options,
                sample, result, candidate_duration, candidate_digest, hr, error);
        };
        const bool measured = sample % 2U == 0U
                                  ? baseline() && candidate()
                                  : candidate() && baseline();
        if (!measured) {
            return finish_failure("compound-measurement");
        }
        if (baseline_digest != candidate_digest) {
            error = "pooled baseline and double-buffered measurement digests differed";
            return finish_failure("compound-paired-digest");
        }
        result.baseline_durations_ns.push_back(baseline_duration);
        result.candidate_durations_ns.push_back(candidate_duration);
        result.correctness_digests.push_back(baseline_digest);
    }

    release_all();
    result.copy_verified = true;
    result.bytes_copied = static_cast<std::uint64_t>(options.payload_bytes) *
                          options.operations_per_sample * options.samples * 2U;
    result.device_removed_reason =
        static_cast<std::uint32_t>(device->GetDeviceRemovedReason());
    if (FAILED(static_cast<HRESULT>(result.device_removed_reason))) {
        fail(result, "compound-device-health",
             static_cast<HRESULT>(result.device_removed_reason),
             "device reported removal after compound measurements");
        return 1;
    }
    result.elapsed_microseconds = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::microseconds>(
            std::chrono::steady_clock::now() - started)
            .count());
    result.stage = "complete";
    result.detail = "paired reusable-pool versus double-buffered preload transfers completed with exact readback on the physical Arc A380 through WSL GPU-PV";
    result.passed = true;
    return 0;
}

int run(const Options &options, Result &result) {
    const auto started = std::chrono::steady_clock::now();
    ComPtr<IDXCoreAdapterFactory> factory;
    HRESULT hr = DXCoreCreateAdapterFactory(
        IID_IDXCoreAdapterFactory, reinterpret_cast<void **>(factory.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-adapter-factory", hr, "DXCoreCreateAdapterFactory failed");
        return 1;
    }

    const GUID attributes[] = {DXCORE_ADAPTER_ATTRIBUTE_D3D12_GRAPHICS};
    ComPtr<IDXCoreAdapterList> adapter_list;
    hr = factory->CreateAdapterList(1, attributes, IID_IDXCoreAdapterList,
                                    reinterpret_cast<void **>(adapter_list.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-adapter-list", hr, "DXCore adapter-list creation failed");
        return 1;
    }

    result.adapters_seen = adapter_list->GetAdapterCount();
    ComPtr<IDXCoreAdapter> selected;
    DXCoreHardwareID selected_id{};
    for (std::uint32_t index = 0; index < result.adapters_seen; ++index) {
        ComPtr<IDXCoreAdapter> adapter;
        hr = adapter_list->GetAdapter(index, IID_IDXCoreAdapter,
                                      reinterpret_cast<void **>(adapter.GetAddressOf()));
        if (FAILED(hr) || !adapter || !adapter->IsValid() ||
            !adapter->IsPropertySupported(DXCoreAdapterProperty::HardwareID)) {
            continue;
        }
        DXCoreHardwareID hardware_id{};
        hr = adapter->GetProperty(DXCoreAdapterProperty::HardwareID, &hardware_id);
        if (FAILED(hr)) {
            continue;
        }
        if (hardware_id.vendorID == options.vendor_id &&
            hardware_id.deviceID == options.device_id) {
            selected = adapter;
            selected_id = hardware_id;
            break;
        }
    }

    if (!selected) {
        result.stage = "select-adapter";
        result.detail = "Requested PCI adapter was not exposed by DXCore";
        return 1;
    }
    result.adapter_name = adapter_description(selected.Get());
    result.vendor_id = selected_id.vendorID;
    result.device_id = selected_id.deviceID;

    ComPtr<ID3D12Device> device;
    hr = D3D12CreateDevice(selected.Get(), D3D_FEATURE_LEVEL_11_0, IID_ID3D12Device,
                           reinterpret_cast<void **>(device.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-device", hr, "D3D12CreateDevice failed for the selected Arc adapter");
        return 1;
    }
    result.device_created = true;

    D3D12_COMMAND_QUEUE_DESC queue_description{};
    queue_description.Type = D3D12_COMMAND_LIST_TYPE_COMPUTE;
    queue_description.Priority = D3D12_COMMAND_QUEUE_PRIORITY_NORMAL;
    queue_description.Flags = D3D12_COMMAND_QUEUE_FLAG_NONE;
    queue_description.NodeMask = 0;

    ComPtr<ID3D12CommandQueue> queue;
    hr = device->CreateCommandQueue(&queue_description, IID_ID3D12CommandQueue,
                                    reinterpret_cast<void **>(queue.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-command-queue", hr, "Compute command-queue creation failed");
        return 1;
    }
    result.queue_created = true;

    if (options.transfer_lab) {
        return run_transfer_lab(device.Get(), queue.Get(), options, result);
    }
    if (options.compound_lab) {
        return run_compound_lab(device.Get(), queue.Get(), options, result);
    }
    ComPtr<ID3D12CommandAllocator> allocator;
    hr = device->CreateCommandAllocator(
        D3D12_COMMAND_LIST_TYPE_COMPUTE, IID_ID3D12CommandAllocator,
        reinterpret_cast<void **>(allocator.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-command-allocator", hr, "Command allocator creation failed");
        return 1;
    }

    ComPtr<ID3D12GraphicsCommandList> command_list;
    hr = device->CreateCommandList(
        0, D3D12_COMMAND_LIST_TYPE_COMPUTE, allocator.Get(), nullptr,
        IID_ID3D12GraphicsCommandList,
        reinterpret_cast<void **>(command_list.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-command-list", hr, "Command-list creation failed");
        return 1;
    }

    D3D12_ROOT_PARAMETER root_parameters[3]{};
    root_parameters[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_SRV;
    root_parameters[0].Descriptor.ShaderRegister = 0;
    root_parameters[0].Descriptor.RegisterSpace = 0;
    root_parameters[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    root_parameters[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_UAV;
    root_parameters[1].Descriptor.ShaderRegister = 0;
    root_parameters[1].Descriptor.RegisterSpace = 0;
    root_parameters[1].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;
    root_parameters[2].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    root_parameters[2].Constants.ShaderRegister = 0;
    root_parameters[2].Constants.RegisterSpace = 0;
    root_parameters[2].Constants.Num32BitValues = 2;
    root_parameters[2].ShaderVisibility = D3D12_SHADER_VISIBILITY_ALL;

    D3D12_ROOT_SIGNATURE_DESC root_signature_description{};
    root_signature_description.NumParameters = 3;
    root_signature_description.pParameters = root_parameters;
    root_signature_description.Flags = D3D12_ROOT_SIGNATURE_FLAG_NONE;

    ComPtr<ID3DBlob> serialized_root_signature;
    ComPtr<ID3DBlob> root_signature_errors;
    hr = D3D12SerializeRootSignature(
        &root_signature_description, D3D_ROOT_SIGNATURE_VERSION_1,
        serialized_root_signature.GetAddressOf(), root_signature_errors.GetAddressOf());
    if (FAILED(hr)) {
        std::string detail = "Root-signature serialization failed";
        if (root_signature_errors && root_signature_errors->GetBufferPointer() != nullptr) {
            detail += ": ";
            detail.append(static_cast<const char *>(root_signature_errors->GetBufferPointer()),
                          root_signature_errors->GetBufferSize());
        }
        fail(result, "serialize-root-signature", hr, detail);
        return 1;
    }

    ComPtr<ID3D12RootSignature> root_signature;
    hr = device->CreateRootSignature(
        0, serialized_root_signature->GetBufferPointer(),
        serialized_root_signature->GetBufferSize(), IID_ID3D12RootSignature,
        reinterpret_cast<void **>(root_signature.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-root-signature", hr, "Compute root-signature creation failed");
        return 1;
    }

    D3D12_COMPUTE_PIPELINE_STATE_DESC pipeline_description{};
    pipeline_description.pRootSignature = root_signature.Get();
    pipeline_description.CS.pShaderBytecode = g_atom_arc_compute_dxil;
    pipeline_description.CS.BytecodeLength = sizeof(g_atom_arc_compute_dxil);

    ComPtr<ID3D12PipelineState> compute_pipeline;
    hr = device->CreateComputePipelineState(
        &pipeline_description, IID_ID3D12PipelineState,
        reinterpret_cast<void **>(compute_pipeline.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-compute-pipeline", hr,
             "Compute pipeline creation failed for the embedded DXIL shader");
        return 1;
    }
    result.compute_pipeline_created = true;

    constexpr std::uint64_t copy_size = 4096;
    D3D12_RESOURCE_DESC buffer_description{};
    buffer_description.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    buffer_description.Alignment = 0;
    buffer_description.Width = copy_size;
    buffer_description.Height = 1;
    buffer_description.DepthOrArraySize = 1;
    buffer_description.MipLevels = 1;
    buffer_description.Format = DXGI_FORMAT_UNKNOWN;
    buffer_description.SampleDesc.Count = 1;
    buffer_description.SampleDesc.Quality = 0;
    buffer_description.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    buffer_description.Flags = D3D12_RESOURCE_FLAG_NONE;

    D3D12_HEAP_PROPERTIES upload_heap{};
    upload_heap.Type = D3D12_HEAP_TYPE_UPLOAD;
    upload_heap.CPUPageProperty = D3D12_CPU_PAGE_PROPERTY_UNKNOWN;
    upload_heap.MemoryPoolPreference = D3D12_MEMORY_POOL_UNKNOWN;
    upload_heap.CreationNodeMask = 1;
    upload_heap.VisibleNodeMask = 1;

    ComPtr<ID3D12Resource> upload;
    hr = device->CreateCommittedResource(
        &upload_heap, D3D12_HEAP_FLAG_NONE, &buffer_description,
        D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(upload.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-upload-resource", hr, "Upload resource creation failed");
        return 1;
    }

    D3D12_HEAP_PROPERTIES readback_heap = upload_heap;
    readback_heap.Type = D3D12_HEAP_TYPE_READBACK;
    ComPtr<ID3D12Resource> readback;
    hr = device->CreateCommittedResource(
        &readback_heap, D3D12_HEAP_FLAG_NONE, &buffer_description,
        D3D12_RESOURCE_STATE_COPY_DEST, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(readback.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-readback-resource", hr, "Readback resource creation failed");
        return 1;
    }

    const std::uint32_t compute_elements = options.compute_elements;
    const std::uint32_t compute_addend = options.compute_addend;
    const std::uint64_t compute_size =
        static_cast<std::uint64_t>(compute_elements) * sizeof(std::uint32_t);
    D3D12_RESOURCE_DESC compute_buffer_description = buffer_description;
    compute_buffer_description.Width = compute_size;
    compute_buffer_description.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;

    D3D12_HEAP_PROPERTIES default_heap = upload_heap;
    default_heap.Type = D3D12_HEAP_TYPE_DEFAULT;
    D3D12_RESOURCE_DESC compute_input_description = compute_buffer_description;
    compute_input_description.Flags = D3D12_RESOURCE_FLAG_NONE;
    ComPtr<ID3D12Resource> compute_input;
    hr = device->CreateCommittedResource(
        &default_heap, D3D12_HEAP_FLAG_NONE, &compute_input_description,
        D3D12_RESOURCE_STATE_COPY_DEST, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(compute_input.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-compute-input", hr, "Compute input resource creation failed");
        return 1;
    }

    ComPtr<ID3D12Resource> compute_upload;
    hr = device->CreateCommittedResource(
        &upload_heap, D3D12_HEAP_FLAG_NONE, &compute_input_description,
        D3D12_RESOURCE_STATE_GENERIC_READ, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(compute_upload.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-compute-upload", hr, "Compute upload resource creation failed");
        return 1;
    }

    ComPtr<ID3D12Resource> compute_output;
    hr = device->CreateCommittedResource(
        &default_heap, D3D12_HEAP_FLAG_NONE, &compute_buffer_description,
        D3D12_RESOURCE_STATE_UNORDERED_ACCESS, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(compute_output.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-compute-output", hr, "Compute output resource creation failed");
        return 1;
    }

    D3D12_RESOURCE_DESC compute_readback_description = compute_buffer_description;
    compute_readback_description.Flags = D3D12_RESOURCE_FLAG_NONE;
    ComPtr<ID3D12Resource> compute_readback;
    hr = device->CreateCommittedResource(
        &readback_heap, D3D12_HEAP_FLAG_NONE, &compute_readback_description,
        D3D12_RESOURCE_STATE_COPY_DEST, nullptr, IID_ID3D12Resource,
        reinterpret_cast<void **>(compute_readback.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-compute-readback", hr,
             "Compute readback resource creation failed");
        return 1;
    }
    result.resources_created = true;
    result.compute_elements = compute_elements;
    result.compute_addend = compute_addend;
    result.compute_input_bytes = compute_size;
    result.compute_output_bytes = compute_size;

    std::vector<std::uint8_t> expected(copy_size);
    std::uint64_t expected_checksum = 0;
    for (std::uint64_t index = 0; index < copy_size; ++index) {
        const auto value = static_cast<std::uint8_t>(((index * 37U) ^ 0xa5U) & 0xffU);
        expected[static_cast<std::size_t>(index)] = value;
        expected_checksum += value;
    }

    void *upload_mapping = nullptr;
    const D3D12_RANGE no_cpu_read{0, 0};
    hr = upload->Map(0, &no_cpu_read, &upload_mapping);
    if (FAILED(hr) || upload_mapping == nullptr) {
        fail(result, "map-upload-resource", hr, "Upload resource mapping failed");
        return 1;
    }
    std::memcpy(upload_mapping, expected.data(), expected.size());
    const D3D12_RANGE upload_written{0, static_cast<SIZE_T>(copy_size)};
    upload->Unmap(0, &upload_written);

    std::vector<std::uint32_t> compute_input_values(compute_elements);
    for (std::uint32_t index = 0; index < compute_elements; ++index) {
        compute_input_values[index] = index * 1664525U;
    }
    void *compute_upload_mapping = nullptr;
    hr = compute_upload->Map(0, &no_cpu_read, &compute_upload_mapping);
    if (FAILED(hr) || compute_upload_mapping == nullptr) {
        fail(result, "map-compute-upload", hr, "Compute upload mapping failed");
        return 1;
    }
    std::memcpy(compute_upload_mapping, compute_input_values.data(),
                static_cast<std::size_t>(compute_size));
    const D3D12_RANGE compute_upload_written{0, static_cast<SIZE_T>(compute_size)};
    compute_upload->Unmap(0, &compute_upload_written);

    command_list->CopyBufferRegion(readback.Get(), 0, upload.Get(), 0, copy_size);
    result.copy_recorded = true;
    result.bytes_copied = copy_size;

    command_list->CopyBufferRegion(compute_input.Get(), 0, compute_upload.Get(), 0,
                                   compute_size);
    D3D12_RESOURCE_BARRIER compute_input_barrier{};
    compute_input_barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    compute_input_barrier.Flags = D3D12_RESOURCE_BARRIER_FLAG_NONE;
    compute_input_barrier.Transition.pResource = compute_input.Get();
    compute_input_barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    compute_input_barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_COPY_DEST;
    compute_input_barrier.Transition.StateAfter =
        D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    command_list->ResourceBarrier(1, &compute_input_barrier);
    result.compute_input_uploaded = true;

    command_list->SetComputeRootSignature(root_signature.Get());
    command_list->SetPipelineState(compute_pipeline.Get());
    command_list->SetComputeRootShaderResourceView(0, compute_input->GetGPUVirtualAddress());
    command_list->SetComputeRootUnorderedAccessView(1, compute_output->GetGPUVirtualAddress());
    const std::uint32_t compute_constants[2] = {compute_addend, compute_elements};
    command_list->SetComputeRoot32BitConstants(2, 2, compute_constants, 0);
    command_list->Dispatch((compute_elements + 63U) / 64U, 1, 1);
    result.compute_dispatched = true;

    D3D12_RESOURCE_BARRIER compute_barrier{};
    compute_barrier.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    compute_barrier.Flags = D3D12_RESOURCE_BARRIER_FLAG_NONE;
    compute_barrier.Transition.pResource = compute_output.Get();
    compute_barrier.Transition.Subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
    compute_barrier.Transition.StateBefore = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    compute_barrier.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    command_list->ResourceBarrier(1, &compute_barrier);
    command_list->CopyBufferRegion(compute_readback.Get(), 0, compute_output.Get(), 0,
                                   compute_size);

    hr = command_list->Close();
    if (FAILED(hr)) {
        fail(result, "close-command-list", hr, "Closing the compute command list failed");
        return 1;
    }
    result.command_list_closed = true;

    ComPtr<ID3D12Fence> fence;
    hr = device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_ID3D12Fence,
                             reinterpret_cast<void **>(fence.GetAddressOf()));
    if (FAILED(hr)) {
        fail(result, "create-fence", hr, "Fence creation failed");
        return 1;
    }

    ID3D12CommandList *command_lists[] = {command_list.Get()};
    queue->ExecuteCommandLists(1, command_lists);
    result.submitted = true;

    constexpr std::uint64_t fence_value = 1;
    hr = queue->Signal(fence.Get(), fence_value);
    if (FAILED(hr)) {
        fail(result, "signal-fence", hr, "Queue fence signal failed");
        return 1;
    }

    const auto deadline = std::chrono::steady_clock::now() +
                          std::chrono::milliseconds(options.timeout_ms);
    while (fence->GetCompletedValue() < fence_value) {
        const HRESULT removed_reason = device->GetDeviceRemovedReason();
        if (FAILED(removed_reason)) {
            result.device_removed_reason = static_cast<std::uint32_t>(removed_reason);
            fail(result, "wait-fence", removed_reason,
                 "Device was removed while waiting for command completion");
            return 1;
        }
        if (std::chrono::steady_clock::now() >= deadline) {
            result.stage = "wait-fence";
            result.detail = "Timed out waiting for the GPU fence";
            return 1;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }

    result.fence_completed = true;
    result.device_removed_reason =
        static_cast<std::uint32_t>(device->GetDeviceRemovedReason());
    if (FAILED(static_cast<HRESULT>(result.device_removed_reason))) {
        fail(result, "device-health", static_cast<HRESULT>(result.device_removed_reason),
             "Device reported a removal reason after fence completion");
        return 1;
    }

    void *readback_mapping = nullptr;
    const D3D12_RANGE readback_range{0, static_cast<SIZE_T>(copy_size)};
    hr = readback->Map(0, &readback_range, &readback_mapping);
    if (FAILED(hr) || readback_mapping == nullptr) {
        fail(result, "map-readback-resource", hr, "Readback resource mapping failed");
        return 1;
    }
    const auto *actual = static_cast<const std::uint8_t *>(readback_mapping);
    std::uint64_t actual_checksum = 0;
    bool copy_matches = true;
    for (std::size_t index = 0; index < expected.size(); ++index) {
        actual_checksum += actual[index];
        if (actual[index] != expected[index]) {
            copy_matches = false;
        }
    }
    const D3D12_RANGE no_cpu_write{0, 0};
    readback->Unmap(0, &no_cpu_write);
    result.copy_checksum = actual_checksum;
    if (!copy_matches || actual_checksum != expected_checksum) {
        result.stage = "verify-copy";
        result.detail = "GPU copy completed but the readback pattern did not match";
        return 1;
    }
    result.copy_verified = true;

    void *compute_mapping = nullptr;
    const D3D12_RANGE compute_readback_range{0, static_cast<SIZE_T>(compute_size)};
    hr = compute_readback->Map(0, &compute_readback_range, &compute_mapping);
    if (FAILED(hr) || compute_mapping == nullptr) {
        fail(result, "map-compute-readback", hr, "Compute readback mapping failed");
        return 1;
    }
    const auto *compute_actual = static_cast<const std::uint32_t *>(compute_mapping);
    std::uint64_t compute_checksum = 0;
    bool compute_matches = true;
    for (std::uint32_t index = 0; index < compute_elements; ++index) {
        const std::uint32_t expected_value =
            (compute_input_values[index] + compute_addend) ^ 0xa5a5a5a5U;
        compute_checksum += compute_actual[index];
        if (compute_actual[index] != expected_value) {
            compute_matches = false;
        }
    }
    compute_readback->Unmap(0, &no_cpu_write);
    result.compute_checksum = compute_checksum;
    if (!compute_matches) {
        result.stage = "verify-compute";
        result.detail = "GPU compute completed but the readback values did not match";
        return 1;
    }
    result.compute_verified = true;

    result.elapsed_microseconds = static_cast<std::uint64_t>(
        std::chrono::duration_cast<std::chrono::microseconds>(
            std::chrono::steady_clock::now() - started)
            .count());
    result.stage = "complete";
    result.detail = "Arc copied 4096 verified bytes and dispatched " +
                    std::to_string(compute_elements) +
                    " verified compute elements through WSL GPU-PV";
    result.passed = true;
    return 0;
}

} // namespace

int main(int argc, char **argv) {
    Options options;
    Result result;
    std::string option_error;
    if (!parse_options(argc, argv, options, option_error)) {
        result.stage = "arguments";
        result.detail = option_error;
        print_result(result);
        return 2;
    }

    const int exit_code = run(options, result);
    print_result(result);
    return exit_code;
}

// Admission follows this worker's full sensor -> 16-bit image -> RGB -> WIC path.
// Keep the same value for metadata-only probing and the actual OS process limit.
#pragma once
#include <algorithm>
#include <cstdint>
static uint32_t raw_child_mib(const libraw_data_t& data,unsigned edge) {
    const uint64_t pixels=uint64_t(data.sizes.raw_width)*data.sizes.raw_height;
    // Bayer: sensor (2), four-channel image (8), RGB result (3), WIC copy (3).
    // X-Trans/multi-channel sensors get extra demosaic workspace.
    const uint64_t per_pixel=(data.idata.filters && data.idata.filters!=9 &&
        !data.idata.is_foveon && data.idata.raw_count==1)?16:24;
    if(!pixels) return 1536;
    // The final RGBA transport buffer scales with requested output, not sensor size.
    const uint64_t output_pixels=edge?std::min<uint64_t>(pixels,uint64_t(edge)*edge):pixels;
    const uint64_t bytes=pixels*per_pixel+output_pixels*4+64ULL*1024*1024;
    return uint32_t(std::min<uint64_t>(1536,(bytes+1024*1024-1)/(1024*1024)));
}
static void raw_memory_report(std::ofstream& output,uint32_t child_mib) {
    const uint32_t total=child_mib+64; // Parent metadata, thumbnail and transport workspace.
    output.write("MORIQMEM",8);
    output.write(reinterpret_cast<const char*>(&total),4);
}

// LibRaw is the decoder. WIC here only resizes an already decoded RGB bitmap.
#define NOMINMAX
#define WIN32_LEAN_AND_MEAN
#include <libraw/libraw.h>
#include <windows.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <algorithm>
#include <cstdint>
#include <fstream>
#include <thread>
#include <vector>
#include <memory>
#include "raw-budget.h"
using Microsoft::WRL::ComPtr;
// 设置 Windows 作业对象的内存和 CPU 限制，父进程退出时结束解码。
static HANDLE guard(DWORD parent,bool probe) {
    HANDLE handle=OpenProcess(SYNCHRONIZE,FALSE,parent);
    if(!handle) ExitProcess(3);
    std::thread([handle]{WaitForSingleObject(handle,INFINITE);ExitProcess(9);}).detach();
    HANDLE job=CreateJobObjectW(nullptr,nullptr);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_PROCESS_MEMORY|JOB_OBJECT_LIMIT_PROCESS_TIME;
    limits.BasicLimitInformation.PerProcessUserTimeLimit.QuadPart=90LL*10000000;
    limits.ProcessMemoryLimit=(probe?192ULL:1536ULL)*1024*1024;
    if(!job||!SetInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits))||!AssignProcessToJobObject(job,GetCurrentProcess())) ExitProcess(3);
    return job;
}
// 执行 RAW 预算探测或 LibRaw 传感器解码，仅用 WIC 缩放已解码像素并输出 RGBA。
int wmain(int argc,wchar_t** argv) {
    const bool probe=argc==6&&wcscmp(argv[5],L"--probe")==0;
    if(argc!=5&&!probe) return 2;
    const unsigned edge=wcstoul(argv[3],nullptr,10);
    if(edge>65535) return 2;
    HANDLE job=guard(wcstoul(argv[4],nullptr,10),probe);
    LibRaw raw;
    raw.imgdata.rawparams.max_raw_memory_mb=1024;
    raw.imgdata.params.use_camera_wb=1;
    raw.imgdata.params.output_color=1; // sRGB
    raw.imgdata.params.output_bps=8;
    raw.imgdata.params.user_qual=3;
    int error=raw.open_file(argv[1]);
    if(error!=LIBRAW_SUCCESS) return 4;
    const uint32_t child_mib=raw_child_mib(raw.imgdata,edge);
    if(probe) {
        std::ofstream output(argv[2],std::ios::binary|std::ios::trunc);
        raw_memory_report(output,child_mib);
        return output.good()?0:7;
    }
    uint32_t original_w=0,original_h=0;
    // Bounded previews can combine Bayer cells before demosaicing. This still
    // develops sensor data; full-resolution viewing/export keeps the full path.
    if(edge>0&&raw.imgdata.idata.filters&&raw.imgdata.idata.filters!=9&&!raw.imgdata.idata.is_foveon&&
       std::max(raw.imgdata.sizes.width,raw.imgdata.sizes.height)>edge*2){
        auto sizing=std::make_unique<LibRaw>();
        if(sizing->open_file(argv[1])==LIBRAW_SUCCESS&&sizing->adjust_sizes_info_only()==LIBRAW_SUCCESS){
            original_w=sizing->imgdata.sizes.iwidth;original_h=sizing->imgdata.sizes.iheight;
            if(original_w&&original_h)raw.imgdata.params.half_size=1;
        }
    }
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    if(!QueryInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits),nullptr))return 3;
    limits.ProcessMemoryLimit=uint64_t(child_mib)*1024*1024;
    if(!SetInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits)))return 3;
    raw.imgdata.rawparams.max_raw_memory_mb=std::min(1024u,child_mib);
    // Always decode sensor data. No embedded JPEG substitution.
    if((error=raw.unpack())!=LIBRAW_SUCCESS||(error=raw.dcraw_process())!=LIBRAW_SUCCESS) return 5;
    libraw_processed_image_t* pixels=raw.dcraw_make_mem_image(&error);
    if(!pixels||error!=LIBRAW_SUCCESS) return 5;
    if(pixels->type!=LIBRAW_IMAGE_BITMAP||pixels->bits!=8||(pixels->colors!=3&&pixels->colors!=1)) {LibRaw::dcraw_clear_mem(pixels);return 5;}
    const uint32_t width=original_w?original_w:pixels->width,height=original_h?original_h:pixels->height;
    uint32_t w=pixels->width,h=pixels->height;
    if(!w||!h||uint64_t(w)*h*4>512ULL*1024*1024) {LibRaw::dcraw_clear_mem(pixels);return 6;}
    std::vector<BYTE> rgb;
    BYTE* data=pixels->data;
    if(pixels->colors==1) {
        rgb.resize(size_t(w)*h*3);
        for(size_t i=0;i<size_t(w)*h;++i) rgb[i*3]=rgb[i*3+1]=rgb[i*3+2]=data[i];
        data=rgb.data();
    }
    if(FAILED(CoInitializeEx(nullptr,COINIT_MULTITHREADED))) return 3;
    ComPtr<IWICImagingFactory> factory;
    if(FAILED(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&factory)))) return 3;
    ComPtr<IWICBitmap> bitmap;
    if(FAILED(factory->CreateBitmapFromMemory(w,h,GUID_WICPixelFormat24bppRGB,w*3,w*h*3,data,&bitmap))) return 5;
    if(edge>0&&std::max(w,h)>edge) {
        const double scale=double(edge)/std::max(w,h);
        w=std::max(1u,unsigned(w*scale));h=std::max(1u,unsigned(h*scale));
    }
    ComPtr<IWICBitmapScaler> scaler;
    if(FAILED(factory->CreateBitmapScaler(&scaler))||FAILED(scaler->Initialize(bitmap.Get(),w,h,WICBitmapInterpolationModeFant))) return 5;
    ComPtr<IWICFormatConverter> converter;
    if(FAILED(factory->CreateFormatConverter(&converter))||FAILED(converter->Initialize(scaler.Get(),GUID_WICPixelFormat32bppRGBA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom))) return 5;
    std::vector<BYTE> rgba(size_t(w)*h*4);
    if(FAILED(converter->CopyPixels(nullptr,w*4,UINT(rgba.size()),rgba.data()))) return 5;
    std::ofstream output(argv[2],std::ios::binary|std::ios::trunc);
    output.write("MORIQIMG",8);
    for(auto value:{width,height,w,h}) output.write(reinterpret_cast<const char*>(&value),4);
    output.write(reinterpret_cast<const char*>(rgba.data()),std::streamsize(rgba.size()));
    LibRaw::dcraw_clear_mem(pixels);
    return output.good()?0:7;
}

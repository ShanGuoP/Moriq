// Small, isolated Windows codec adapter. No network or database access.
#define NOMINMAX
#include <windows.h>
#include <wincodec.h>
#include <wrl/client.h>
#include <propvarutil.h>
#include <algorithm>
#include <cstdint>
#include <climits>
#include <fstream>
#include <vector>
#include <thread>
using Microsoft::WRL::ComPtr;
static void guard(DWORD parent) {
    HANDLE handle=OpenProcess(SYNCHRONIZE,FALSE,parent);
    if (!handle) ExitProcess(3);
    std::thread([handle] { WaitForSingleObject(handle,INFINITE); ExitProcess(9); }).detach();
    HANDLE job=CreateJobObjectW(nullptr,nullptr);
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_PROCESS_MEMORY|JOB_OBJECT_LIMIT_PROCESS_TIME;
    limits.BasicLimitInformation.PerProcessUserTimeLimit.QuadPart=30LL*10000000;
    limits.ProcessMemoryLimit=768ULL*1024*1024;
    if (!job || !SetInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits)) || !AssignProcessToJobObject(job,GetCurrentProcess())) ExitProcess(3);
    // The process owns the job handle for its entire lifetime.
}
int wmain(int argc,wchar_t** argv) {
    if(argc!=5&&argc!=9) return 2;
    const bool region=argc==9;
    const unsigned edge=wcstoul(argv[3],nullptr,10);
    if(edge>8192) return 2;
    guard(wcstoul(argv[4],nullptr,10));
    if(FAILED(CoInitializeEx(nullptr,COINIT_MULTITHREADED))) return 3;
    ComPtr<IWICImagingFactory> factory;
    if(FAILED(CoCreateInstance(CLSID_WICImagingFactory,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&factory)))) return 3;
    ComPtr<IWICBitmapDecoder> decoder;
    if(FAILED(factory->CreateDecoderFromFilename(argv[1],nullptr,GENERIC_READ,WICDecodeMetadataCacheOnDemand,&decoder))) return 4;
    ComPtr<IWICBitmapFrameDecode> frame;
    if(FAILED(decoder->GetFrame(0,&frame))) return 5;
    UINT original_w=0,original_h=0;
    if(FAILED(frame->GetSize(&original_w,&original_h)) || !original_w || !original_h) return 6;
    unsigned orientation=1;
    ComPtr<IWICMetadataQueryReader> metadata;
    if(SUCCEEDED(frame->GetMetadataQueryReader(&metadata))) {
        for(auto name:{L"/app1/ifd/{ushort=274}",L"/ifd/{ushort=274}"}) {
            PROPVARIANT value; PropVariantInit(&value);
            if(SUCCEEDED(metadata->GetMetadataByName(name,&value))) {
                if(value.vt==VT_UI2) orientation=value.uiVal;
                if(value.vt==VT_UI4) orientation=value.ulVal;
            }
            PropVariantClear(&value);
        }
    }
    const bool swapped=orientation>=5 && orientation<=8;
    const uint32_t full_w=swapped?original_h:original_w,full_h=swapped?original_w:original_h;
    std::ofstream output(argv[2],std::ios::binary|std::ios::trunc);
    if(!output) return 7;
    output.write("MORIQIMG",8);
    output.write(reinterpret_cast<const char*>(&full_w),4);
    output.write(reinterpret_cast<const char*>(&full_h),4);
    if(edge==0) {
        const uint32_t zero=0;output.write(reinterpret_cast<const char*>(&zero),4);output.write(reinterpret_cast<const char*>(&zero),4);
        return output.good()?0:7;
    }
    ComPtr<IWICBitmapSource> color_source;
    if(FAILED(frame.As(&color_source)))return 5;
    ComPtr<IWICColorContext> source_context,destination_context;
    ComPtr<IWICColorTransform> color_transform;
    UINT contexts=0;
    if(SUCCEEDED(frame->GetColorContexts(0,nullptr,&contexts))&&contexts>0){
        if(FAILED(factory->CreateColorContext(&source_context))||FAILED(factory->CreateColorContext(&destination_context)))return 5;
        IWICColorContext* contexts_array[]={source_context.Get()};UINT actual=0;
        if(FAILED(frame->GetColorContexts(1,contexts_array,&actual))||FAILED(destination_context->InitializeFromExifColorSpace(1))||FAILED(factory->CreateColorTransformer(&color_transform))||FAILED(color_transform->Initialize(frame.Get(),source_context.Get(),destination_context.Get(),GUID_WICPixelFormat32bppRGBA))||FAILED(color_transform.As(&color_source)))return 5;
    }
    double scale=region?1.0:std::min(1.0,static_cast<double>(edge)/std::max(original_w,original_h));
    UINT w=std::max(1u,static_cast<UINT>(original_w*scale)),h=std::max(1u,static_cast<UINT>(original_h*scale));
    ComPtr<IWICBitmapScaler> scaler;
    if(FAILED(factory->CreateBitmapScaler(&scaler)) || FAILED(scaler->Initialize(color_source.Get(),w,h,WICBitmapInterpolationModeFant))) return 5;
    WICBitmapTransformOptions transform=WICBitmapTransformRotate0;
    switch(orientation) {
        case 2:transform=WICBitmapTransformFlipHorizontal;break;
        case 3:transform=WICBitmapTransformRotate180;break;
        case 4:transform=WICBitmapTransformFlipVertical;break;
        case 5:transform=static_cast<WICBitmapTransformOptions>(WICBitmapTransformRotate90|WICBitmapTransformFlipHorizontal);break;
        case 6:transform=WICBitmapTransformRotate90;break;
        case 7:transform=static_cast<WICBitmapTransformOptions>(WICBitmapTransformRotate270|WICBitmapTransformFlipHorizontal);break;
        case 8:transform=WICBitmapTransformRotate270;break;
    }
    ComPtr<IWICBitmapFlipRotator> rotator;
    if(FAILED(factory->CreateBitmapFlipRotator(&rotator)) || FAILED(rotator->Initialize(scaler.Get(),transform))) return 5;
    if(FAILED(rotator->GetSize(&w,&h))) return 5;
    ComPtr<IWICBitmapSource> region_source;
    if(FAILED(rotator.As(&region_source)))return 5;
    ComPtr<IWICBitmapClipper> clipper;ComPtr<IWICBitmapScaler> region_scaler;
    if(region){
        const uint64_t x=wcstoull(argv[5],nullptr,10),y=wcstoull(argv[6],nullptr,10),rw=wcstoull(argv[7],nullptr,10),rh=wcstoull(argv[8],nullptr,10);
        if(!rw||!rh||x+rw>w||y+rh>h||x>INT_MAX||y>INT_MAX||rw>INT_MAX||rh>INT_MAX)return 6;
        WICRect rectangle{int(x),int(y),int(rw),int(rh)};
        if(FAILED(factory->CreateBitmapClipper(&clipper))||FAILED(clipper->Initialize(rotator.Get(),&rectangle)))return 5;
        const double ratio=std::min(1.0,double(edge)/std::max(rw,rh));w=std::max(1u,unsigned(rw*ratio));h=std::max(1u,unsigned(rh*ratio));
        if(FAILED(factory->CreateBitmapScaler(&region_scaler))||FAILED(region_scaler->Initialize(clipper.Get(),w,h,WICBitmapInterpolationModeFant))||FAILED(region_scaler.As(&region_source)))return 5;
    }
    ComPtr<IWICFormatConverter> converter;
    if(FAILED(factory->CreateFormatConverter(&converter)) || FAILED(converter->Initialize(region_source.Get(),GUID_WICPixelFormat32bppRGBA,WICBitmapDitherTypeNone,nullptr,0,WICBitmapPaletteTypeCustom))) return 5;
    const uint64_t size=uint64_t(w)*h*4;
    if(!w || !h || w>edge || h>edge || size>256ULL*1024*1024) return 6;
    std::vector<BYTE> rgba(static_cast<size_t>(size));
    if(FAILED(converter->CopyPixels(nullptr,w*4,static_cast<UINT>(size),rgba.data()))) return 5;
    output.write(reinterpret_cast<const char*>(&w),4);output.write(reinterpret_cast<const char*>(&h),4);
    output.write(reinterpret_cast<const char*>(rgba.data()),static_cast<std::streamsize>(size));
    return output.good()?0:7;
}

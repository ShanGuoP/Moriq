// Standalone LibRaw decoder for Unix builds. No system image codec dependency.
#include <libraw/libraw.h>
#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <fstream>
#include <thread>
#include <chrono>
#include <vector>
#include <atomic>
#include <cstring>
#include "raw-budget.h"
#include <unistd.h>
#include <sys/resource.h>
#ifdef __APPLE__
#include <mach/mach.h>
#endif
static std::atomic<uint64_t> memory_limit{1536ULL*1024*1024};
// 设置便携 RAW 解码的数据内存上限，并同步监控线程使用的预算。
static void set_memory_limit(uint32_t mib){
    memory_limit=uint64_t(mib)*1024*1024;
    rlimit memory{memory_limit.load(),1536ULL*1024*1024};if(setrlimit(RLIMIT_DATA,&memory)!=0)std::_Exit(3);
}
// 设置 CPU 和初始内存限制，并监视父进程及平台支持的驻留内存。
static void guard(pid_t parent,bool probe){
    rlimit cpu{90,90};if(setrlimit(RLIMIT_CPU,&cpu)!=0)std::_Exit(3);
    set_memory_limit(probe?192:1536);
    std::thread([parent]{for(;;){if(getppid()!=parent)std::_Exit(9);
#ifdef __APPLE__
        mach_task_basic_info info{};mach_msg_type_number_t count=MACH_TASK_BASIC_INFO_COUNT;
        if(task_info(mach_task_self(),MACH_TASK_BASIC_INFO,reinterpret_cast<task_info_t>(&info),&count)==KERN_SUCCESS&&info.resident_size>memory_limit.load())std::_Exit(6);
#endif
        std::this_thread::sleep_for(std::chrono::milliseconds(100));}}).detach();
}
// 解析便携 RAW 工作参数，执行预算探测或传感器解码，并写出受限尺寸的 RGBA 协议。
int main(int argc,char**argv){
    const bool probe=argc==6&&std::strcmp(argv[5],"--probe")==0;
    if(argc!=5&&!probe)return 2;const unsigned edge=std::strtoul(argv[3],nullptr,10);if(edge>65535)return 2;guard(std::strtol(argv[4],nullptr,10),probe);
    LibRaw raw;raw.imgdata.rawparams.max_raw_memory_mb=1024;raw.imgdata.params.use_camera_wb=1;raw.imgdata.params.output_color=1;raw.imgdata.params.output_bps=8;raw.imgdata.params.user_qual=3;
    if(raw.open_file(argv[1])!=LIBRAW_SUCCESS)return 4;
    const uint32_t child_mib=raw_child_mib(raw.imgdata,edge);
    if(probe){std::ofstream output(argv[2],std::ios::binary);raw_memory_report(output,child_mib);return output.good()?0:7;}
    set_memory_limit(child_mib);raw.imgdata.rawparams.max_raw_memory_mb=std::min(1024u,child_mib);
    if(raw.unpack()!=LIBRAW_SUCCESS||raw.dcraw_process()!=LIBRAW_SUCCESS)return 5;
    int error=0;libraw_processed_image_t* image=raw.dcraw_make_mem_image(&error);if(!image||error!=LIBRAW_SUCCESS)return 5;
    if(image->type!=LIBRAW_IMAGE_BITMAP||image->bits!=8||(image->colors!=3&&image->colors!=1))return 5;
    const uint32_t width=image->width,height=image->height;uint32_t w=width,h=height;
    if(!w||!h||uint64_t(w)*h*4>512ULL*1024*1024)return 6;
    if(edge&&std::max(w,h)>edge){const double scale=double(edge)/std::max(w,h);w=std::max(1u,unsigned(w*scale));h=std::max(1u,unsigned(h*scale));}
    std::vector<unsigned char> rgba(size_t(w)*h*4);
    for(uint32_t y=0;y<h;++y){const uint32_t y0=uint64_t(y)*height/h,y1=std::max(y0+1,uint32_t(uint64_t(y+1)*height/h));
        for(uint32_t x=0;x<w;++x){const uint32_t x0=uint64_t(x)*width/w,x1=std::max(x0+1,uint32_t(uint64_t(x+1)*width/w));uint64_t channels[3]={0,0,0};
            for(uint32_t sy=y0;sy<y1;++sy)for(uint32_t sx=x0;sx<x1;++sx){const auto* pixel=image->data+(size_t(sy)*width+sx)*image->colors;for(unsigned c=0;c<3;++c)channels[c]+=pixel[image->colors==1?0:c];}
            const uint64_t count=uint64_t(x1-x0)*(y1-y0);auto* pixel=&rgba[(size_t(y)*w+x)*4];for(unsigned c=0;c<3;++c)pixel[c]=(channels[c]+count/2)/count;pixel[3]=255;
        }
    }
    LibRaw::dcraw_clear_mem(image);std::ofstream output(argv[2],std::ios::binary);const uint32_t sizes[]={width,height,w,h};output.write("MORIQIMG",8);output.write(reinterpret_cast<const char*>(sizes),sizeof(sizes));output.write(reinterpret_cast<const char*>(rgba.data()),rgba.size());output.flush();return output.good()?0:7;
}

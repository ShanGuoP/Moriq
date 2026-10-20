// Embedded Windows playback surface. All decoding is provided by bundled FFmpeg.
#define NOMINMAX
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <commctrl.h>
#include <mmsystem.h>
extern "C" {
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/channel_layout.h>
#include <libavutil/imgutils.h>
#include <libavutil/display.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
}
#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <deque>
#include <memory>
#include <mutex>
#include <string>
#include <fstream>
#include <thread>
#include <vector>
#include <iostream>
#include <sstream>
static std::mutex viewport_mutex;
static double view_x=0,view_y=0,view_w=0,view_h=0;
static std::atomic<bool> stopping{false},paused{false},muted{false};
static std::atomic<long long> seek_ms{-1},position_ms{0},duration_ms{0};
static std::mutex pixels_mutex;
static std::vector<uint8_t> pixels;
static int picture_w=0,picture_h=0;
static HWND surface=nullptr,owner=nullptr,seekbar=nullptr,pause_button=nullptr,mute_button=nullptr;
static DWORD owner_pid=0;
static bool dragging=false;
static std::ofstream diagnostics;
static std::atomic<unsigned> rendered{0};
static BOOL CALLBACK find_owner(HWND w,LPARAM) {
    DWORD pid=0;GetWindowThreadProcessId(w,&pid);
    if(pid==owner_pid&&IsWindowVisible(w)&&!GetWindow(w,GW_OWNER)) {owner=w;return FALSE;}return TRUE;
}
static void paint(HWND w) {
    PAINTSTRUCT ps;HDC dc=BeginPaint(w,&ps);RECT r;GetClientRect(w,&r);r.bottom=std::max(0L,r.bottom-42);
    FillRect(dc,&r,static_cast<HBRUSH>(GetStockObject(BLACK_BRUSH)));
    std::lock_guard<std::mutex> lock(pixels_mutex);
    if(!pixels.empty()) {
        double scale=std::min(double(r.right)/picture_w,double(r.bottom)/picture_h);
        int width=int(picture_w*scale),height=int(picture_h*scale);
        BITMAPINFO info{};info.bmiHeader.biSize=sizeof(BITMAPINFOHEADER);info.bmiHeader.biWidth=picture_w;info.bmiHeader.biHeight=-picture_h;info.bmiHeader.biPlanes=1;info.bmiHeader.biBitCount=32;info.bmiHeader.biCompression=BI_RGB;
        SetStretchBltMode(dc,HALFTONE);
        StretchDIBits(dc,(r.right-width)/2,(r.bottom-height)/2,width,height,0,0,picture_w,picture_h,pixels.data(),&info,DIB_RGB_COLORS,SRCCOPY);
    }
    EndPaint(w,&ps);
}
static LRESULT CALLBACK procedure(HWND w,UINT msg,WPARAM a,LPARAM b) {
    switch(msg) {
    case WM_PAINT:paint(w);return 0;
    case WM_ERASEBKGND:return 1;
    case WM_COMMAND:
        if(LOWORD(a)==1){paused=!paused.load();SetWindowTextW(pause_button,paused?L"播放":L"暂停");}
        if(LOWORD(a)==2){muted=!muted.load();SetWindowTextW(mute_button,muted?L"取消静音":L"静音");}
        return 0;
    case WM_HSCROLL:
        if(reinterpret_cast<HWND>(b)==seekbar) {
            const int event=LOWORD(a);dragging=event==TB_THUMBTRACK;
            if(event==TB_ENDTRACK||event==TB_THUMBPOSITION||event==TB_PAGEUP||event==TB_PAGEDOWN||event==TB_LINEUP||event==TB_LINEDOWN)
                seek_ms=SendMessageW(seekbar,TBM_GETPOS,0,0)*duration_ms.load()/10000;
        }return 0;
    case WM_TIMER: {
        if(!IsWindow(owner)){stopping=true;DestroyWindow(w);return 0;}
        RECT r;GetClientRect(owner,&r);double dpi=double(GetDpiForWindow(owner))/96.0;
        double x,y,vw,vh;{std::lock_guard<std::mutex> lock(viewport_mutex);x=view_x;y=view_y;vw=view_w;vh=view_h;}
        if(vw<=0||vh<=0){ShowWindow(w,SW_HIDE);return 0;}
        int left=std::clamp(int(x*dpi),0,int(r.right)),top=std::clamp(int(y*dpi),0,int(r.bottom));
        int width=std::clamp(int(vw*dpi),0,int(r.right)-left),height=std::clamp(int(vh*dpi),0,int(r.bottom)-top);
        SetWindowPos(w,HWND_TOP,left,top,width,height,SWP_NOACTIVATE|SWP_SHOWWINDOW);
        MoveWindow(pause_button,8,height-36,64,28,TRUE);MoveWindow(mute_button,80,height-36,86,28,TRUE);MoveWindow(seekbar,174,height-36,std::max(10,width-182),28,TRUE);
        if(!dragging&&duration_ms>0)SendMessageW(seekbar,TBM_SETPOS,TRUE,position_ms.load()*10000/duration_ms.load());
        InvalidateRect(w,nullptr,FALSE);return 0;
    }
    case WM_DESTROY:stopping=true;PostQuitMessage(0);return 0;
    }
    return DefWindowProcW(w,msg,a,b);
}
static std::string utf8(const wchar_t* value) {
    int n=WideCharToMultiByte(CP_UTF8,0,value,-1,nullptr,0,nullptr,nullptr);
    std::string s(size_t(n),0);WideCharToMultiByte(CP_UTF8,0,value,-1,s.data(),n,nullptr,nullptr);s.pop_back();return s;
}
static AVCodecContext* codec(AVFormatContext* f,int index) {
    if(index<0)return nullptr;
    const AVCodec* c=avcodec_find_decoder(f->streams[index]->codecpar->codec_id);if(!c)return nullptr;
    AVCodecContext* ctx=avcodec_alloc_context3(c);if(!ctx)return nullptr;
    if(avcodec_parameters_to_context(ctx,f->streams[index]->codecpar)<0){avcodec_free_context(&ctx);return nullptr;}
    ctx->thread_count=2;
    if(avcodec_open2(ctx,c,nullptr)<0){avcodec_free_context(&ctx);return nullptr;}return ctx;
}
static int interrupt(void*) {return stopping?1:0;}
struct Sound {WAVEHDR header{};std::vector<uint8_t> samples;};
static void decode(const std::wstring path) {
    auto fail=[](const char* message){diagnostics<<"error "<<message<<std::endl;stopping=true;PostMessageW(surface,WM_CLOSE,0,0);};
    AVFormatContext* format=avformat_alloc_context();if(!format){fail("allocation");return;}
    format->interrupt_callback={interrupt,nullptr};
    AVDictionary* options=nullptr;av_dict_set(&options,"protocol_whitelist","file,pipe",0);av_dict_set(&options,"format_whitelist","mov,matroska,webm,avi,mpegts",0);
    int result=avformat_open_input(&format,utf8(path.c_str()).c_str(),nullptr,&options);av_dict_free(&options);
    if(result<0||avformat_find_stream_info(format,nullptr)<0){if(format)avformat_close_input(&format);fail("open");return;}
    int vi=av_find_best_stream(format,AVMEDIA_TYPE_VIDEO,-1,-1,nullptr,0),ai=av_find_best_stream(format,AVMEDIA_TYPE_AUDIO,-1,-1,nullptr,0);
    AVCodecContext* vc=codec(format,vi),*ac=codec(format,ai);
    if(!vc){avcodec_free_context(&ac);avformat_close_input(&format);fail("video codec");return;}
    duration_ms=std::max<int64_t>(0,format->duration/1000);
    SwsContext* scaler=nullptr;SwrContext* resampler=nullptr;HWAVEOUT audio=nullptr;
    WAVEFORMATEX wav{};wav.wFormatTag=WAVE_FORMAT_PCM;wav.nChannels=2;wav.nSamplesPerSec=48000;wav.wBitsPerSample=16;wav.nBlockAlign=4;wav.nAvgBytesPerSec=192000;
    if(ac&&waveOutOpen(&audio,WAVE_MAPPER,&wav,0,0,CALLBACK_NULL)==MMSYSERR_NOERROR){
        AVChannelLayout stereo=AV_CHANNEL_LAYOUT_STEREO;
        if(swr_alloc_set_opts2(&resampler,&stereo,AV_SAMPLE_FMT_S16,48000,&ac->ch_layout,ac->sample_fmt,ac->sample_rate,0,nullptr)<0||swr_init(resampler)<0){swr_free(&resampler);waveOutClose(audio);audio=nullptr;}
    }
    std::deque<std::unique_ptr<Sound>> sounds;
    auto release=[&]{while(!sounds.empty()&&(sounds.front()->header.dwFlags&WHDR_DONE)){waveOutUnprepareHeader(audio,&sounds.front()->header,sizeof(WAVEHDR));sounds.pop_front();}};
    auto reset_audio=[&]{if(audio){waveOutReset(audio);for(auto& s:sounds)waveOutUnprepareHeader(audio,&s->header,sizeof(WAVEHDR));sounds.clear();}};
    AVPacket* packet=av_packet_alloc();AVFrame* frame=av_frame_alloc();
    using Clock=std::chrono::steady_clock;auto origin=Clock::now();long long base=0,audio_base=-1,seek_floor=0;bool was_paused=false,eof=false,seek_preview=false;int draining=-1;
    diagnostics<<"started audio="<<(audio?1:0)<<" duration_ms="<<duration_ms.load()<<std::endl;
    while(!stopping){
        if(paused!=was_paused){auto now=Clock::now();if(paused){base+=std::chrono::duration_cast<std::chrono::milliseconds>(now-origin).count();if(audio)waveOutPause(audio);}else{origin=now;if(audio)waveOutRestart(audio);}was_paused=paused;}
        long long target=seek_ms.exchange(-1);
        if(target>=0){
            int64_t timestamp=target*1000+(format->start_time==AV_NOPTS_VALUE?0:format->start_time);
            if(av_seek_frame(format,-1,timestamp,AVSEEK_FLAG_BACKWARD)>=0){avcodec_flush_buffers(vc);if(ac)avcodec_flush_buffers(ac);reset_audio();if(resampler){swr_close(resampler);swr_init(resampler);}base=target;seek_floor=target;audio_base=-1;origin=Clock::now();eof=false;draining=-1;seek_preview=paused;}
        }
        if((paused&&!seek_preview)||eof){std::this_thread::sleep_for(std::chrono::milliseconds(10));continue;}
        release();
        if(draining<0&&av_read_frame(format,packet)<0)draining=0;
        if(draining>1){eof=true;diagnostics<<"ended frames="<<rendered.load()<<" position_ms="<<position_ms.load()<<std::endl;continue;}
        const bool video=draining>=0?draining==0:packet->stream_index==vi;
        AVCodecContext* decoder=draining>=0?(draining==0?vc:ac):video?vc:packet->stream_index==ai?ac:nullptr;
        const bool flush=draining>=0;if(flush)++draining;
        if(!decoder){av_packet_unref(packet);continue;}
        result=avcodec_send_packet(decoder,flush?nullptr:packet);av_packet_unref(packet);
        if(result<0)continue;
        while(!stopping&&avcodec_receive_frame(decoder,frame)>=0){
            if(seek_ms>=0){av_frame_unref(frame);break;}
            if(video){
                int64_t timestamp=frame->best_effort_timestamp;
                long long pts=timestamp==AV_NOPTS_VALUE?base+std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now()-origin).count():int64_t(timestamp*av_q2d(format->streams[vi]->time_base)*1000);
                if(format->start_time!=AV_NOPTS_VALUE)pts-=format->start_time/1000;
                if(pts+50<seek_floor){av_frame_unref(frame);continue;}
                const auto wait_start=Clock::now();
                while(!stopping&&!paused&&seek_ms<0){
                    long long clock=base+std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now()-origin).count();
                    if(audio&&audio_base>=0){MMTIME time{};time.wType=TIME_SAMPLES;if(waveOutGetPosition(audio,&time,sizeof(time))==MMSYSERR_NOERROR&&time.wType==TIME_SAMPLES)clock=audio_base+int64_t(time.u.sample)*1000/48000;}
                    if(pts<=clock+3||Clock::now()-wait_start>std::chrono::milliseconds(200))break;
                    std::this_thread::sleep_for(std::chrono::milliseconds(2));
                }
                // Bound playback upload size independently of the source frame dimensions.
                int w=frame->width,h=frame->height;if(w<=0||h<=0||int64_t(w)*h>536870912){stopping=true;break;}
                double factor=std::min(1.0,1920.0/std::max(w,h));w=std::max(1,int(w*factor));h=std::max(1,int(h*factor));
                scaler=sws_getCachedContext(scaler,frame->width,frame->height,AVPixelFormat(frame->format),w,h,AV_PIX_FMT_BGRA,SWS_BILINEAR,nullptr,nullptr,nullptr);
                if(!scaler){stopping=true;break;}
                std::vector<uint8_t> next(size_t(w)*h*4);uint8_t* output[]={next.data()};int stride[]={w*4};
                sws_scale(scaler,frame->data,frame->linesize,0,frame->height,output,stride);
                auto side=av_packet_side_data_get(format->streams[vi]->codecpar->coded_side_data,format->streams[vi]->codecpar->nb_coded_side_data,AV_PKT_DATA_DISPLAYMATRIX);
                int turns=0;if(side&&side->size>=9*sizeof(int32_t)){double angle=-av_display_rotation_get(reinterpret_cast<int32_t*>(side->data));if(std::isfinite(angle))turns=(int(std::lround(angle/90))+4)%4;}
                if(turns){int rw=turns%2?h:w,rh=turns%2?w:h;std::vector<uint8_t> rotated(size_t(rw)*rh*4);for(int y=0;y<h;++y)for(int x=0;x<w;++x){int dx=turns==1?h-1-y:turns==2?w-1-x:y;int dy=turns==1?x:turns==2?h-1-y:w-1-x;memcpy(&rotated[(size_t(dy)*rw+dx)*4],&next[(size_t(y)*w+x)*4],4);}next.swap(rotated);w=rw;h=rh;}
                {std::lock_guard<std::mutex> lock(pixels_mutex);pixels.swap(next);picture_w=w;picture_h=h;}
                position_ms=std::max(0LL,pts);
                if(++rendered%12==0)diagnostics<<"frame "<<rendered.load()<<" position_ms="<<position_ms.load()<<std::endl;
                if(seek_preview&&paused){seek_preview=false;av_frame_unref(frame);break;}
            }else if(audio&&resampler&&!paused){
                while(sounds.size()>=12&&!stopping&&!paused&&seek_ms<0){release();std::this_thread::sleep_for(std::chrono::milliseconds(2));}
                auto sound=std::make_unique<Sound>();int capacity=int(av_rescale_rnd(swr_get_delay(resampler,ac->sample_rate)+frame->nb_samples,48000,ac->sample_rate,AV_ROUND_UP));
                if(capacity<=0||capacity>480000){av_frame_unref(frame);continue;}
                sound->samples.resize(size_t(capacity)*4);uint8_t* out[]={sound->samples.data()};
                int count=swr_convert(resampler,out,capacity,const_cast<const uint8_t**>(frame->extended_data),frame->nb_samples);
                if(count>0){
                    if(audio_base<0){audio_base=frame->best_effort_timestamp==AV_NOPTS_VALUE?position_ms.load():int64_t(frame->best_effort_timestamp*av_q2d(format->streams[ai]->time_base)*1000);if(format->start_time!=AV_NOPTS_VALUE)audio_base-=format->start_time/1000;}
                    waveOutSetVolume(audio,muted?0:0xFFFFFFFF);sound->header.lpData=reinterpret_cast<LPSTR>(sound->samples.data());sound->header.dwBufferLength=count*4;
                    if(waveOutPrepareHeader(audio,&sound->header,sizeof(WAVEHDR))==MMSYSERR_NOERROR){if(waveOutWrite(audio,&sound->header,sizeof(WAVEHDR))==MMSYSERR_NOERROR)sounds.push_back(std::move(sound));else waveOutUnprepareHeader(audio,&sound->header,sizeof(WAVEHDR));}
                }
            }
            av_frame_unref(frame);
        }
    }
    reset_audio();if(audio)waveOutClose(audio);swr_free(&resampler);sws_freeContext(scaler);av_frame_free(&frame);av_packet_free(&packet);avcodec_free_context(&vc);avcodec_free_context(&ac);avformat_close_input(&format);
    PostMessageW(surface,WM_CLOSE,0,0);
}
int wmain(int argc,wchar_t** argv) {
    if(argc!=4)return 2;diagnostics.open(argv[3],std::ios::trunc);owner_pid=wcstoul(argv[2],nullptr,10);EnumWindows(find_owner,0);if(!owner)return 3;
    HANDLE parent=OpenProcess(SYNCHRONIZE,FALSE,owner_pid);if(!parent)return 3;
    std::thread([parent]{WaitForSingleObject(parent,INFINITE);ExitProcess(9);}).detach();
    HANDLE job=CreateJobObjectW(nullptr,nullptr);JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_PROCESS_MEMORY;limits.ProcessMemoryLimit=1024ULL*1024*1024;
    if(!job||!SetInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits))||!AssignProcessToJobObject(job,GetCurrentProcess()))return 3;
    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    INITCOMMONCONTROLSEX controls{sizeof(controls),ICC_BAR_CLASSES};InitCommonControlsEx(&controls);
    WNDCLASSW cls{};cls.lpfnWndProc=procedure;cls.hInstance=GetModuleHandleW(nullptr);cls.lpszClassName=L"MORIQVideoSurface";cls.hCursor=LoadCursorW(nullptr,IDC_ARROW);RegisterClassW(&cls);
    surface=CreateWindowExW(0,cls.lpszClassName,L"Moriq Space 内置播放器",WS_CHILD|WS_CLIPCHILDREN,0,0,1,1,owner,nullptr,cls.hInstance,nullptr);if(!surface)return 3;
    std::thread([]{std::string line;while(std::getline(std::cin,line)){if(line.size()>160)break;std::istringstream input(line);std::string command;double x,y,w,h;if(input>>command>>x>>y>>w>>h&&command=="RECT"&&std::isfinite(x)&&std::isfinite(y)&&std::isfinite(w)&&std::isfinite(h)&&x>=0&&y>=0&&w>=0&&h>=0&&x<=100000&&y<=100000&&w<=100000&&h<=100000){std::lock_guard<std::mutex> lock(viewport_mutex);view_x=x;view_y=y;view_w=w;view_h=h;}}stopping=true;PostMessageW(surface,WM_CLOSE,0,0);}).detach();
    pause_button=CreateWindowW(L"BUTTON",L"暂停",WS_CHILD|WS_VISIBLE,0,0,64,28,surface,reinterpret_cast<HMENU>(1),cls.hInstance,nullptr);
    mute_button=CreateWindowW(L"BUTTON",L"静音",WS_CHILD|WS_VISIBLE,0,0,86,28,surface,reinterpret_cast<HMENU>(2),cls.hInstance,nullptr);
    seekbar=CreateWindowW(TRACKBAR_CLASSW,L"",WS_CHILD|WS_VISIBLE|TBS_HORZ,0,0,100,28,surface,reinterpret_cast<HMENU>(3),cls.hInstance,nullptr);SendMessageW(seekbar,TBM_SETRANGE,TRUE,MAKELPARAM(0,10000));
    SetTimer(surface,1,33,nullptr);std::thread worker(decode,std::wstring(argv[1]));
    MSG msg;while(GetMessageW(&msg,nullptr,0,0)>0){TranslateMessage(&msg);DispatchMessageW(&msg);}stopping=true;worker.join();return 0;
}

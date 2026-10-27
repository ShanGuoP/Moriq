// Isolated FFmpeg decoder. RGBA frames go to the Rust/Iced renderer over stdout.
#define NOMINMAX
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <fcntl.h>
#include <io.h>
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
static std::atomic<bool> stopping{false},paused{false},muted{false};
static std::atomic<long long> seek_ms{-1},position_ms{0},duration_ms{0};
static std::ofstream diagnostics;
static std::mutex diagnostics_mutex;
static std::atomic<unsigned> rendered{0};
static const auto process_started=std::chrono::steady_clock::now();
// 返回播放进程启动以来的毫秒数，用于播放诊断计时。
static long long elapsed_ms(){return std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now()-process_started).count();}
// 按固定小端帧协议输出尺寸、位置、时长和 RGBA 像素，刷新管道供界面读取。
static bool write_frame(int w,int h,long long pts,const std::vector<uint8_t>& pixels) {
    // Fixed little-endian header; Windows targets are little endian.
    const uint32_t width=w,height=h;const uint64_t position=std::max(0LL,pts),duration=std::max(0LL,duration_ms.load());
    std::cout.write("MORIQFR1",8);
    std::cout.write(reinterpret_cast<const char*>(&width),4);std::cout.write(reinterpret_cast<const char*>(&height),4);
    std::cout.write(reinterpret_cast<const char*>(&position),8);std::cout.write(reinterpret_cast<const char*>(&duration),8);
    std::cout.write(reinterpret_cast<const char*>(pixels.data()),pixels.size());std::cout.flush();
    return bool(std::cout);
}
// 把 Windows 宽字符文件路径转换为 FFmpeg 使用的 UTF-8。
static std::string utf8(const wchar_t* value) {
    int n=WideCharToMultiByte(CP_UTF8,0,value,-1,nullptr,0,nullptr,nullptr);
    std::string s(size_t(n),0);WideCharToMultiByte(CP_UTF8,0,value,-1,s.data(),n,nullptr,nullptr);s.pop_back();return s;
}
// 为指定媒体流创建解码上下文，并设置有界解码线程数；失败时释放已分配资源。
static AVCodecContext* codec(AVFormatContext* f,int index) {
    if(index<0)return nullptr;
    const AVCodec* c=avcodec_find_decoder(f->streams[index]->codecpar->codec_id);if(!c)return nullptr;
    AVCodecContext* ctx=avcodec_alloc_context3(c);if(!ctx)return nullptr;
    if(avcodec_parameters_to_context(ctx,f->streams[index]->codecpar)<0){avcodec_free_context(&ctx);return nullptr;}
    ctx->thread_count=ctx->codec_type==AVMEDIA_TYPE_VIDEO?std::clamp(int(GetActiveProcessorCount(ALL_PROCESSOR_GROUPS))/2,2,8):2;
    if(avcodec_open2(ctx,c,nullptr)<0){avcodec_free_context(&ctx);return nullptr;}return ctx;
}
// 把停止标志传给 FFmpeg 的阻塞读取中断回调。
static int interrupt(void*) {return stopping?1:0;}
struct Sound {WAVEHDR header{};std::vector<uint8_t> samples;};
// 读取本地音视频流，处理跳转、暂停和音量；按时钟输出有界视频帧并回收解码资源。
static void decode(const std::wstring path) {
    auto fail=[](const char* message){{std::lock_guard<std::mutex> lock(diagnostics_mutex);diagnostics<<"error "<<message<<std::endl;}stopping=true;};
    AVFormatContext* format=avformat_alloc_context();if(!format){fail("allocation");return;}
    format->interrupt_callback={interrupt,nullptr};
    AVDictionary* options=nullptr;av_dict_set(&options,"protocol_whitelist","file,pipe",0);av_dict_set(&options,"format_whitelist","mov,matroska,webm,avi,mpegts",0);
    int result=avformat_open_input(&format,utf8(path.c_str()).c_str(),nullptr,&options);av_dict_free(&options);
    if(result<0){if(format)avformat_close_input(&format);fail("open");return;}
    // MOV/MP4 already carries complete codec parameters in its sample tables.
    // Only probe packets when the container has not supplied those parameters.
    bool known_video=false,known_audio=true;
    for(unsigned i=0;i<format->nb_streams;++i){
        const auto* p=format->streams[i]->codecpar;
        if(p->codec_type==AVMEDIA_TYPE_VIDEO&&p->codec_id!=AV_CODEC_ID_NONE&&p->width>0&&p->height>0&&p->extradata_size>0)known_video=true;
        if(p->codec_type==AVMEDIA_TYPE_AUDIO&&p->sample_rate<=0)known_audio=false;
    }
    if((std::string(format->iformat->name).find("mov")==std::string::npos||!known_video||!known_audio)&&avformat_find_stream_info(format,nullptr)<0){avformat_close_input(&format);fail("stream info");return;}
    int vi=av_find_best_stream(format,AVMEDIA_TYPE_VIDEO,-1,-1,nullptr,0),ai=av_find_best_stream(format,AVMEDIA_TYPE_AUDIO,-1,-1,nullptr,0);
    AVCodecContext* vc=codec(format,vi),*ac=codec(format,ai);
    if(!vc){avcodec_free_context(&ac);avformat_close_input(&format);fail("video codec");return;}
    duration_ms=format->duration>0?format->duration/1000:0;
    if(duration_ms<=0){for(unsigned i=0;i<format->nb_streams;++i){const auto* stream=format->streams[i];if(stream->duration>0&&stream->duration!=AV_NOPTS_VALUE)duration_ms=std::max<int64_t>(duration_ms.load(),av_rescale_q(stream->duration,stream->time_base,AVRational{1,1000}));}}
    SwsContext* scaler=nullptr;SwrContext* resampler=nullptr;HWAVEOUT audio=nullptr;
    WAVEFORMATEX wav{};wav.wFormatTag=WAVE_FORMAT_PCM;wav.nChannels=2;wav.nSamplesPerSec=48000;wav.wBitsPerSample=16;wav.nBlockAlign=4;wav.nAvgBytesPerSec=192000;
    if(ac&&waveOutOpen(&audio,WAVE_MAPPER,&wav,0,0,CALLBACK_NULL)==MMSYSERR_NOERROR){
        AVChannelLayout stereo=AV_CHANNEL_LAYOUT_STEREO;
        if(swr_alloc_set_opts2(&resampler,&stereo,AV_SAMPLE_FMT_S16,48000,&ac->ch_layout,ac->sample_fmt,ac->sample_rate,0,nullptr)<0||swr_init(resampler)<0){swr_free(&resampler);waveOutClose(audio);audio=nullptr;}
    }
    std::deque<std::unique_ptr<Sound>> sounds;size_t sound_bytes=0;
    auto release=[&]{while(!sounds.empty()&&(sounds.front()->header.dwFlags&WHDR_DONE)){waveOutUnprepareHeader(audio,&sounds.front()->header,sizeof(WAVEHDR));sound_bytes-=sounds.front()->samples.size();sounds.pop_front();}};
    auto reset_audio=[&]{if(audio){waveOutReset(audio);for(auto& s:sounds)waveOutUnprepareHeader(audio,&s->header,sizeof(WAVEHDR));sounds.clear();sound_bytes=0;}};
    AVPacket* packet=av_packet_alloc();AVFrame* frame=av_frame_alloc();
    using Clock=std::chrono::steady_clock;auto origin=Clock::now();long long base=0,audio_base=-1,seek_floor=0;bool was_paused=false,was_muted=false,eof=false,seek_preview=false;int draining=-1;long long last_export=-1000;
    {std::lock_guard<std::mutex> lock(diagnostics_mutex);diagnostics<<"started audio="<<(audio?1:0)<<" duration_ms="<<duration_ms.load()<<std::endl;}
    while(!stopping){
        if(audio&&muted!=was_muted){was_muted=muted;waveOutSetVolume(audio,was_muted?0:0xFFFFFFFF);}
        if(paused!=was_paused){auto now=Clock::now();if(paused){base+=std::chrono::duration_cast<std::chrono::milliseconds>(now-origin).count();if(audio)waveOutPause(audio);}else{origin=now;if(audio)waveOutRestart(audio);}was_paused=paused;}
        long long target=seek_ms.exchange(-1);
        if(target>=0){
            int64_t timestamp=target*1000+(format->start_time==AV_NOPTS_VALUE?0:format->start_time);
            if(av_seek_frame(format,-1,timestamp,AVSEEK_FLAG_BACKWARD)>=0){avcodec_flush_buffers(vc);if(ac)avcodec_flush_buffers(ac);reset_audio();if(resampler){swr_close(resampler);swr_init(resampler);}base=target;seek_floor=target;audio_base=-1;origin=Clock::now();eof=false;draining=-1;last_export=-1000;seek_preview=paused;}
        }
        if((paused&&!seek_preview)||eof){std::this_thread::sleep_for(std::chrono::milliseconds(10));continue;}
        release();
        if(draining<0&&av_read_frame(format,packet)<0)draining=0;
        if(draining>1){eof=true;{std::lock_guard<std::mutex> lock(diagnostics_mutex);diagnostics<<"ended frames="<<rendered.load()<<" position_ms="<<position_ms.load()<<" elapsed_ms="<<elapsed_ms()<<std::endl;}continue;}
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
                // Keep the media clock moving on expensive high-bit-depth clips.
                // Decode reference frames, but avoid converting already-late frames.
                const auto wall_clock=base+std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now()-origin).count();
                if(rendered>0&&!seek_preview&&pts+80<wall_clock){position_ms=std::max(0LL,pts);av_frame_unref(frame);continue;}
                const auto wait_start=Clock::now();
                while(!stopping&&!paused&&seek_ms<0){
                    long long clock=base+std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now()-origin).count();
                    if(audio&&audio_base>=0&&!sounds.empty()){
                        MMTIME time{};time.wType=TIME_SAMPLES;
                        if(waveOutGetPosition(audio,&time,sizeof(time))==MMSYSERR_NOERROR&&time.wType==TIME_SAMPLES){
                            const auto audio_clock=audio_base+int64_t(time.u.sample)*1000/48000;
                            // A single demux thread must not wait on an empty audio
                            // queue while the next audio packet is behind video packets.
                            if(std::llabs(audio_clock-clock)<50)clock=audio_clock;
                        }
                    }
                    if(pts<=clock+3||Clock::now()-wait_start>std::chrono::milliseconds(200))break;
                    std::this_thread::sleep_for(std::chrono::milliseconds(2));
                }
                // Decode all reference frames, upload at most 30 per second.
                if(!seek_preview&&pts-last_export<30){av_frame_unref(frame);continue;}
                // Bound playback upload size independently of the source frame dimensions.
                int w=frame->width,h=frame->height;if(w<=0||h<=0||int64_t(w)*h>536870912){stopping=true;break;}
                double factor=std::min(1.0,1920.0/std::max(w,h));w=std::max(1,int(w*factor));h=std::max(1,int(h*factor));
                scaler=sws_getCachedContext(scaler,frame->width,frame->height,AVPixelFormat(frame->format),w,h,AV_PIX_FMT_RGBA,SWS_BILINEAR,nullptr,nullptr,nullptr);
                if(!scaler){stopping=true;break;}
                std::vector<uint8_t> next(size_t(w)*h*4);uint8_t* output[]={next.data()};int stride[]={w*4};
                sws_scale(scaler,frame->data,frame->linesize,0,frame->height,output,stride);
                auto side=av_packet_side_data_get(format->streams[vi]->codecpar->coded_side_data,format->streams[vi]->codecpar->nb_coded_side_data,AV_PKT_DATA_DISPLAYMATRIX);
                int turns=0;if(side&&side->size>=9*sizeof(int32_t)){double angle=-av_display_rotation_get(reinterpret_cast<int32_t*>(side->data));if(std::isfinite(angle))turns=(int(std::lround(angle/90))+4)%4;}
                if(turns){int rw=turns%2?h:w,rh=turns%2?w:h;std::vector<uint8_t> rotated(size_t(rw)*rh*4);for(int y=0;y<h;++y)for(int x=0;x<w;++x){int dx=turns==1?h-1-y:turns==2?w-1-x:y;int dy=turns==1?x:turns==2?h-1-y:w-1-x;memcpy(&rotated[(size_t(dy)*rw+dx)*4],&next[(size_t(y)*w+x)*4],4);}next.swap(rotated);w=rw;h=rh;}
                if(!write_frame(w,h,pts,next)){stopping=true;break;}last_export=pts;
                position_ms=std::max(0LL,pts);
                const auto frame_number=++rendered;
                if(frame_number==1){std::lock_guard<std::mutex> lock(diagnostics_mutex);diagnostics<<"first_frame_ms="<<elapsed_ms()<<std::endl;}
                if(frame_number%12==0){std::lock_guard<std::mutex> lock(diagnostics_mutex);diagnostics<<"frame "<<rendered.load()<<" position_ms="<<position_ms.load()<<" elapsed_ms="<<elapsed_ms()<<std::endl;}
                if(seek_preview&&paused){seek_preview=false;av_frame_unref(frame);break;}
            }else if(audio&&resampler&&!paused){
                // MOV can interleave many small audio packets before the next
                // video packet. Bound actual bytes instead of stalling at 12 packets.
                while((sounds.size()>=256||sound_bytes>=8*1024*1024)&&!stopping&&!paused&&seek_ms<0){release();std::this_thread::sleep_for(std::chrono::milliseconds(2));}
                auto sound=std::make_unique<Sound>();int capacity=int(av_rescale_rnd(swr_get_delay(resampler,ac->sample_rate)+frame->nb_samples,48000,ac->sample_rate,AV_ROUND_UP));
                if(capacity<=0||capacity>480000){av_frame_unref(frame);continue;}
                sound->samples.resize(size_t(capacity)*4);uint8_t* out[]={sound->samples.data()};
                int count=swr_convert(resampler,out,capacity,const_cast<const uint8_t**>(frame->extended_data),frame->nb_samples);
                if(count>0){
                    if(audio_base<0){audio_base=frame->best_effort_timestamp==AV_NOPTS_VALUE?position_ms.load():int64_t(frame->best_effort_timestamp*av_q2d(format->streams[ai]->time_base)*1000);if(format->start_time!=AV_NOPTS_VALUE)audio_base-=format->start_time/1000;}
                    waveOutSetVolume(audio,muted?0:0xFFFFFFFF);sound->header.lpData=reinterpret_cast<LPSTR>(sound->samples.data());sound->header.dwBufferLength=count*4;
                    if(waveOutPrepareHeader(audio,&sound->header,sizeof(WAVEHDR))==MMSYSERR_NOERROR){if(waveOutWrite(audio,&sound->header,sizeof(WAVEHDR))==MMSYSERR_NOERROR){sound_bytes+=sound->samples.size();sounds.push_back(std::move(sound));}else waveOutUnprepareHeader(audio,&sound->header,sizeof(WAVEHDR));}
                }
            }
            av_frame_unref(frame);
        }
    }
    reset_audio();if(audio)waveOutClose(audio);swr_free(&resampler);sws_freeContext(scaler);av_frame_free(&frame);av_packet_free(&packet);avcodec_free_context(&vc);avcodec_free_context(&ac);avformat_close_input(&format);

}
// 建立父进程保护和控制通道，启动播放解码并在停止后等待工作线程结束。
int wmain(int argc,wchar_t** argv) {
    if(argc!=4)return 2;diagnostics.open(argv[3],std::ios::trunc);
    HANDLE parent=OpenProcess(SYNCHRONIZE,FALSE,wcstoul(argv[2],nullptr,10));if(!parent)return 3;
    std::thread([parent]{WaitForSingleObject(parent,INFINITE);ExitProcess(9);}).detach();
    HANDLE job=CreateJobObjectW(nullptr,nullptr);JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_PROCESS_MEMORY;limits.ProcessMemoryLimit=1024ULL*1024*1024;
    if(!job||!SetInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits))||!AssignProcessToJobObject(job,GetCurrentProcess()))return 3;
    _setmode(_fileno(stdout),_O_BINARY);
    std::thread([]{
        std::string line;
        while(std::getline(std::cin,line)){
            if(line.size()>160)break;
            std::istringstream input(line);std::string command;long long value;
            if(!(input>>command>>value))continue;
            if(command=="PAUSE"&&(value==0||value==1))paused=value!=0;
            if(command=="MUTE"&&(value==0||value==1))muted=value!=0;
            if(command=="SEEK"&&value>=0&&value<=duration_ms.load())seek_ms=value;
            {std::lock_guard<std::mutex> lock(diagnostics_mutex);diagnostics<<"control "<<command<<" value="<<value<<" elapsed_ms="<<elapsed_ms()<<std::endl;}
        }
        stopping=true;
    }).detach();
    decode(std::wstring(argv[1]));return 0;
}

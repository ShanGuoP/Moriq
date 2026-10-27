// Job lifetime and resource limits also cover abrupt application exit.
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>
#include <thread>
// 按 Windows 命令行规则转义单个参数，保留引号前与末尾反斜线。
static std::wstring quote(const wchar_t* argument){
    std::wstring result=L"\"";unsigned slashes=0;
    for(const wchar_t* p=argument;*p;++p){if(*p==L'\\'){++slashes;continue;}if(*p==L'\"'){result.append(slashes*2+1,L'\\');result+=L'\"';}else{result.append(slashes,L'\\');result+=*p;}slashes=0;}
    result.append(slashes*2,L'\\');return result+L"\"";
}
// 在作业对象内启动工具并限制资源；父进程退出时终止整组进程并回收句柄。
int wmain(int argc,wchar_t** argv){
    if(argc<3)return 2;
    const bool ai=argc>3&&std::wstring(argv[2])==L"--ai";
    const int first=ai?3:2;
    HANDLE parent=OpenProcess(SYNCHRONIZE,FALSE,wcstoul(argv[1],nullptr,10));if(!parent)return 3;
    HANDLE job=CreateJobObjectW(nullptr,nullptr);JOBOBJECT_EXTENDED_LIMIT_INFORMATION limits{};
    limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE|JOB_OBJECT_LIMIT_PROCESS_MEMORY|JOB_OBJECT_LIMIT_PROCESS_TIME;
    limits.ProcessMemoryLimit=1024ULL*1024*1024;limits.BasicLimitInformation.PerProcessUserTimeLimit.QuadPart=90LL*10000000;
    if(ai){limits.ProcessMemoryLimit=6ULL*1024*1024*1024;limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE|JOB_OBJECT_LIMIT_PROCESS_MEMORY;}
    if(!job||!SetInformationJobObject(job,JobObjectExtendedLimitInformation,&limits,sizeof(limits)))return 3;
    std::wstring line;for(int i=first;i<argc;++i){if(i>first)line+=L' ';line+=quote(argv[i]);}
    STARTUPINFOW start{};start.cb=sizeof(start);start.dwFlags=STARTF_USESTDHANDLES;start.hStdInput=GetStdHandle(STD_INPUT_HANDLE);start.hStdOutput=GetStdHandle(STD_OUTPUT_HANDLE);start.hStdError=GetStdHandle(STD_ERROR_HANDLE);
    PROCESS_INFORMATION process{};
    if(!CreateProcessW(argv[first],line.data(),nullptr,nullptr,TRUE,CREATE_SUSPENDED|CREATE_NO_WINDOW,nullptr,nullptr,&start,&process))return 4;
    if(!AssignProcessToJobObject(job,process.hProcess)){TerminateProcess(process.hProcess,3);WaitForSingleObject(process.hProcess,INFINITE);CloseHandle(process.hThread);CloseHandle(process.hProcess);CloseHandle(job);return 3;}
    ResumeThread(process.hThread);CloseHandle(process.hThread);
    HANDLE handles[]={process.hProcess,parent};DWORD wait=WaitForMultipleObjects(2,handles,FALSE,INFINITE);DWORD code=9;
    if(wait==WAIT_OBJECT_0)GetExitCodeProcess(process.hProcess,&code);else TerminateJobObject(job,9);
    WaitForSingleObject(process.hProcess,INFINITE);CloseHandle(process.hProcess);CloseHandle(parent);CloseHandle(job);return int(code);
}

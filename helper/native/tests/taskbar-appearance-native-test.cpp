#include <windows.h>
#include <sddl.h>
#include <aclapi.h>
#include <cassert>
#include <iostream>
#include <string>
#include <vector>
#include "../taskbar-appearance-policy.h"
#include "../taskbar-shared-security.h"

struct Snapshot {DWORD state;HRESULT error;DWORD backgrounds;DWORD explorer;};
struct Options {DWORD mode;DWORD opacity;DWORD tint;LONG showBorder;};
struct Wire {DWORD magic;DWORD owner;volatile LONG enabled;Snapshot snapshot;Options options;volatile LONG optionsRevision;volatile LONG attached;};
static_assert(sizeof(Wire)==52);
std::wstring MapName(DWORD pid) {
    HANDLE token{};assert(OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&token));
    DWORD size{};GetTokenInformation(token,TokenUser,nullptr,0,&size);
    std::vector<BYTE> buffer(size);assert(GetTokenInformation(token,TokenUser,buffer.data(),size,&size));CloseHandle(token);
    LPWSTR sid{};assert(ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER*>(buffer.data())->User.Sid,&sid));
    std::wstring result=L"Local\\ConvenientWindow.Taskbar.v2."+std::wstring(sid)+L"."+std::to_wstring(pid);LocalFree(sid);return result;
}
#if defined(CW_TASKBAR_LEGACY_FIXTURE)
HMODULE module{};
extern "C" __declspec(dllexport) void __cdecl CWTaskbarUpdate() {}
extern "C" __declspec(dllexport) void __cdecl CWTaskbarClose() {}
STDAPI DllGetClassObject(REFCLSID,REFIID,void**) {return CLASS_E_CLASSNOTAVAILABLE;}
#if defined(CW_TASKBAR_INCOMPATIBLE_FIXTURE)
extern "C" __declspec(dllexport) DWORD __cdecl CWTaskbarWireVersion() {return 3;}
#endif
extern "C" __declspec(dllexport) LRESULT CALLBACK TaskbarHook(int,WPARAM,LPARAM value) {
    auto message=reinterpret_cast<CWPSTRUCT*>(value);
    auto mapping=OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,MapName(static_cast<DWORD>(message->wParam)).c_str());
    if(!mapping) return 0;
    auto wire=static_cast<Wire*>(MapViewOfFile(mapping,FILE_MAP_ALL_ACCESS,0,0,sizeof(Wire)));
    if(wire) {
        wire->snapshot={0,S_OK,0,GetCurrentProcessId()};
        if(GetPropW(message->hwnd,L"ConvenientWindow.Taskbar.Module.v1")!=reinterpret_cast<HANDLE>(module)) {
            wire->snapshot={6,TaskbarModulePolicy::ProductVersionConflict(),0,GetCurrentProcessId()};
        }
        InterlockedExchange(&wire->attached,1);UnmapViewOfFile(wire);
    }
    CloseHandle(mapping);return 0;
}
BOOL WINAPI DllMain(HINSTANCE instance,DWORD reason,LPVOID) {if(reason==DLL_PROCESS_ATTACH) module=instance;return TRUE;}
#else
void VerifySharedSecurity() {
    HANDLE token{}; assert(OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY|TOKEN_DUPLICATE,&token));
    DWORD size{}; GetTokenInformation(token,TokenUser,nullptr,0,&size);
    std::vector<BYTE> bytes(size); assert(GetTokenInformation(token,TokenUser,bytes.data(),size,&size));
    auto user=reinterpret_cast<TOKEN_USER*>(bytes.data())->User.Sid;
    LPWSTR userText{}; assert(ConvertSidToStringSidW(user,&userText));
    TaskbarModulePolicy::UserScopedMediumSecurity security(userText); LocalFree(userText); assert(security.valid());
    BOOL present{},defaulted{}; PACL acl{};
    assert(GetSecurityDescriptorDacl(security.get()->lpSecurityDescriptor,&present,&acl,&defaulted));
    assert(present && acl && acl->AceCount==2);
    for(DWORD index=0;index<acl->AceCount;++index) {
        void* raw{};assert(GetAce(acl,index,&raw));auto ace=static_cast<ACCESS_ALLOWED_ACE*>(raw);
        assert(ace->Header.AceType==ACCESS_ALLOWED_ACE_TYPE);
        auto sid=reinterpret_cast<PSID>(&ace->SidStart);
        assert(EqualSid(sid,user) || IsWellKnownSid(sid,WinLocalSystemSid));
    }
    const auto name=L"Local\\ConvenientWindow.Taskbar.SecurityTest."+std::to_wstring(GetCurrentProcessId());
    HANDLE mapping=CreateFileMappingW(INVALID_HANDLE_VALUE,security.get(),PAGE_READWRITE,0,64,name.c_str());assert(mapping);
    HANDLE opened=OpenFileMappingW(FILE_MAP_WRITE,FALSE,name.c_str());assert(opened);CloseHandle(opened);
    PSECURITY_DESCRIPTOR actual{};PACL sacl{};
    assert(GetSecurityInfo(mapping,SE_KERNEL_OBJECT,LABEL_SECURITY_INFORMATION,nullptr,nullptr,nullptr,&sacl,&actual)==ERROR_SUCCESS);
    assert(sacl && sacl->AceCount==1);void* raw{};assert(GetAce(sacl,0,&raw));
    auto label=static_cast<SYSTEM_MANDATORY_LABEL_ACE*>(raw);auto sid=reinterpret_cast<PSID>(&label->SidStart);
    assert(label->Header.AceType==SYSTEM_MANDATORY_LABEL_ACE_TYPE);
    assert(label->Mask==SYSTEM_MANDATORY_LABEL_NO_WRITE_UP);
    assert(*GetSidSubAuthority(sid,0)==SECURITY_MANDATORY_MEDIUM_RID);LocalFree(actual);
    HANDLE lowToken{};PSID lowSid{};
    assert(DuplicateTokenEx(token,TOKEN_QUERY|TOKEN_IMPERSONATE|TOKEN_ADJUST_DEFAULT,nullptr,SecurityImpersonation,TokenImpersonation,&lowToken));
    assert(ConvertStringSidToSidW(L"S-1-16-4096",&lowSid));TOKEN_MANDATORY_LABEL low{{lowSid,SE_GROUP_INTEGRITY}};
    assert(SetTokenInformation(lowToken,TokenIntegrityLevel,&low,sizeof(low)+GetLengthSid(lowSid)));
    assert(ImpersonateLoggedOnUser(lowToken));HANDLE denied=OpenFileMappingW(FILE_MAP_WRITE,FALSE,name.c_str());const auto error=GetLastError();
    assert(RevertToSelf());if(denied) CloseHandle(denied);assert(!denied && error==ERROR_ACCESS_DENIED);
    LocalFree(lowSid);CloseHandle(lowToken);CloseHandle(token);CloseHandle(mapping);
}
int wmain(int argc,wchar_t** argv) {
    if(argc==2 && std::wstring_view(argv[1])==L"owner") {Sleep(5000);return 0;}
    if(argc<3) {std::cerr<<"usage: native-test current-dll resident-dll [stale]\n";return 2;}
    VerifySharedSecurity();
    auto current=LoadLibraryW(argv[1]);auto resident=LoadLibraryW(argv[2]);
    assert(current && resident && current!=resident);
    const auto mode=argc>3?std::wstring_view(argv[3]):L"reuse";
    const bool stale=mode==L"stale";
    const bool incompatible=mode==L"incompatible";
    const bool conflict=mode==L"conflict";
    assert(TaskbarModulePolicy::IsClassicWindows10Build(10,18362));
    assert(TaskbarModulePolicy::IsClassicWindows10Build(10,22000));
    assert(!TaskbarModulePolicy::IsClassicWindows10Build(10,22621));
    assert(TaskbarModulePolicy::ClassicAccentStateForMode(0)==2);
    assert(TaskbarModulePolicy::ClassicAccentStateForMode(1)==4);
    assert(TaskbarModulePolicy::ClassicAccentStateForMode(2)==1);
    assert(TaskbarModulePolicy::ClassicGradientColor(0xAABBCC,100)==0xFFCCBBAA);
    assert(TaskbarModulePolicy::ClassicGradientColor(0xAABBCC,0)==0x00CCBBAA);
    assert(TaskbarModulePolicy::ShouldClearStaleMarker(reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(0x1234)),current,false));
    assert(!TaskbarModulePolicy::ShouldClearStaleMarker(reinterpret_cast<HANDLE>(resident),current,true));
    assert(TaskbarModulePolicy::IsLegacyV2Name(L"taskbar-0123456789abcdef.dll"));
    assert(!TaskbarModulePolicy::IsLegacyV2Name(L"taskbar-xxxxxxxxxxxxxxxx.dll"));
    assert(TaskbarModulePolicy::ClassicEntryBelongsToResident(42,42));
    assert(!TaskbarModulePolicy::ClassicEntryBelongsToResident(41,42));
    assert(!TaskbarModulePolicy::ClassicEntryBelongsToResident(0,42));
    HWND window=CreateWindowExW(0,L"STATIC",L"taskbar-regression",0,0,0,1,1,nullptr,nullptr,GetModuleHandleW(nullptr),nullptr);
    assert(window);
    const auto marker=stale?reinterpret_cast<HANDLE>(static_cast<ULONG_PTR>(0x1234)):reinterpret_cast<HANDLE>(resident);
    assert(SetPropW(window,L"ConvenientWindow.Taskbar.Module.v1",marker));
    auto mapping=CreateFileMappingW(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,sizeof(Wire),MapName(GetCurrentProcessId()).c_str());assert(mapping);
    auto wire=static_cast<Wire*>(MapViewOfFile(mapping,FILE_MAP_ALL_ACCESS,0,0,sizeof(Wire)));assert(wire);
    // A real window property and v2 mapping drive the production hook. Disabled
    // intent isolates this test from Explorer and XAML rendering.
    *wire={0x43575432,GetCurrentProcessId(),0,{1,S_OK,0,GetCurrentProcessId()},{0,0,0,1},2,0};
    using MarkClassicDirty=void(__cdecl*)(UINT,HWND);
    auto markClassicDirty=reinterpret_cast<MarkClassicDirty>(GetProcAddress(current,"CWTaskbarMarkClassicDirty"));assert(markClassicDirty);
    // Calling the dirty export on a non-taskbar window must be harmless and
    // must not inject into Explorer or perform any composition update.
    markClassicDirty(WM_THEMECHANGED,window);
    CWPSTRUCT message{0,GetCurrentProcessId(),RegisterWindowMessageW(L"ConvenientWindow.Taskbar.Attach.v2"),window};
    using Hook=LRESULT(CALLBACK*)(int,WPARAM,LPARAM);
    auto hook=reinterpret_cast<Hook>(GetProcAddress(current,"TaskbarHook"));assert(hook);
    assert(TaskbarModulePolicy::ShouldForwardClassicDirty(resident,current,true,true));
    assert(!TaskbarModulePolicy::ShouldForwardClassicDirty(resident,current,true,false));
    assert(!TaskbarModulePolicy::ShouldForwardClassicDirty(resident,current,false,true));
    assert(!TaskbarModulePolicy::ShouldForwardClassicDirty(current,current,true,true));
    PROCESS_INFORMATION child{};HANDLE childMapping{};Wire* childWire{};
    if(conflict) {
        wchar_t exe[32768]{};assert(GetModuleFileNameW(nullptr,exe,32768));
        auto command=L"\""+std::wstring(exe)+L"\" owner";STARTUPINFOW startup{};startup.cb=sizeof(startup);
        assert(CreateProcessW(nullptr,command.data(),nullptr,nullptr,FALSE,CREATE_NO_WINDOW,nullptr,nullptr,&startup,&child));
        childMapping=CreateFileMappingW(INVALID_HANDLE_VALUE,nullptr,PAGE_READWRITE,0,sizeof(Wire),MapName(child.dwProcessId).c_str());assert(childMapping);
        childWire=static_cast<Wire*>(MapViewOfFile(childMapping,FILE_MAP_ALL_ACCESS,0,0,sizeof(Wire)));assert(childWire);
        *childWire={0x43575432,child.dwProcessId,0,{1,S_OK,0,0},{0,0,0,1},2,0};
        using Attach=HRESULT(__cdecl*)(DWORD,HWND);
        auto attach=reinterpret_cast<Attach>(GetProcAddress(resident,"CWTaskbarAttachOwner"));assert(attach);
        assert(SUCCEEDED(attach(child.dwProcessId,window)));
        assert(childWire->attached==1);
    }
    hook(HC_ACTION,0,reinterpret_cast<LPARAM>(&message));
    const auto actual=GetPropW(window,L"ConvenientWindow.Taskbar.Module.v1");
    const bool expectedState=incompatible?wire->snapshot.state==6 && wire->snapshot.error==TaskbarModulePolicy::ProductVersionConflict():
        conflict?wire->snapshot.state==4 && wire->snapshot.error==HRESULT_FROM_WIN32(ERROR_BUSY):
        wire->snapshot.error!=TaskbarModulePolicy::ProductVersionConflict();
    const bool ok=wire->attached==1 && expectedState && actual==reinterpret_cast<HANDLE>(stale?current:resident);
    std::cout<<"attached="<<wire->attached<<" error=0x"<<std::hex<<static_cast<unsigned long>(wire->snapshot.error)
        <<" marker="<<(actual==reinterpret_cast<HANDLE>(resident)?"resident":"current")<<"\n";
    InterlockedExchange(&wire->enabled,-1);Sleep(450);
    if(child.hProcess) {
        // This is only the synthetic process created by this isolated test.
        TerminateProcess(child.hProcess,0);WaitForSingleObject(child.hProcess,1000);
        CloseHandle(child.hThread);CloseHandle(child.hProcess);
        UnmapViewOfFile(childWire);CloseHandle(childMapping);
    }
    UnmapViewOfFile(wire);CloseHandle(mapping);RemovePropW(window,L"ConvenientWindow.Taskbar.Module.v1");DestroyWindow(window);
    // Modules may be pinned by the tested hook; exit the isolated test process.
    return ok?0:1;
}

#endif

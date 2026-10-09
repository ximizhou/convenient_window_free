// Independently implemented against Windows SDK XAML Diagnostics interfaces.
// No third-party taskbar implementation is included in this component.
#include <windows.h>
#include "taskbar-appearance-policy.h"
#include "taskbar-shared-security.h"
#include <sddl.h>
#include <tlhelp32.h>
#include <xamlom.h>
#undef GetCurrentTime
#include <winrt/base.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.UI.h>
#include <winrt/Windows.UI.Core.h>
#include <winrt/Windows.System.h>
#include <winrt/Windows.UI.Xaml.h>
#include <winrt/Windows.UI.Xaml.Media.h>
#include <winrt/Windows.UI.Xaml.Shapes.h>
#include <atomic>
#include <algorithm>
#include <memory>
#include <mutex>
#include <string>
#include <thread>
#include <unordered_map>
#include <vector>

namespace xaml = winrt::Windows::UI::Xaml;
namespace media = winrt::Windows::UI::Xaml::Media;
namespace core = winrt::Windows::UI::Core;
constexpr DWORD kMagic = 0x43575432;
// Fresh class ID for this component, unrelated to other taskbar tools.
const CLSID kTapClass = {0xd1a5b30b,0x123a,0x4e26,{0x97,0x39,0x9c,0xe2,0x75,0x4e,0xd4,0x6b}};
struct Snapshot { DWORD state; HRESULT error; DWORD backgrounds; DWORD explorer; };
enum : DWORD { kModeTransparent = 0, kModeAcrylic = 1, kModeSolid = 2 };
struct AppearanceOptions { DWORD mode; DWORD opacity; DWORD tint; LONG showBorder; };
struct Wire { DWORD magic; DWORD owner; volatile LONG enabled; Snapshot snapshot; AppearanceOptions options; volatile LONG optionsRevision; volatile LONG attached; };
static_assert(sizeof(AppearanceOptions) == 16);
static_assert(sizeof(Snapshot) == 16);
static_assert(sizeof(Wire) == 52);
HMODULE gModule;
UINT gAttachMessage;
std::atomic_bool gDiagnosticsStarted{false};
bool IsClassicWindows();
void MarkClassicChange(UINT message,HWND hwnd);
struct Owner;
void ClassicUpdate(bool enabled,const AppearanceOptions& requested,const std::shared_ptr<Owner>& owner,Snapshot* result);
extern std::atomic_bool gClassicDirty;

struct Handle {
    HANDLE value{};
    explicit Handle(HANDLE h = nullptr):value(h){}
    ~Handle(){if(value && value != INVALID_HANDLE_VALUE) CloseHandle(value);}
};
std::wstring UserScope() {
    HANDLE token{};
    if(!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) winrt::throw_last_error();
    Handle close(token);
    DWORD size{}; GetTokenInformation(token, TokenUser, nullptr, 0, &size);
    std::vector<BYTE> bytes(size);
    if(!GetTokenInformation(token, TokenUser, bytes.data(), size, &size)) winrt::throw_last_error();
    LPWSTR sid{};
    if(!ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER*>(bytes.data())->User.Sid, &sid)) winrt::throw_last_error();
    std::wstring result(sid); LocalFree(sid); return result;
}
std::wstring MapName(DWORD pid) { return L"Local\\ConvenientWindow.Taskbar.v2." + UserScope() + L"." + std::to_wstring(pid); }
struct Owner {
    Handle mapping, process;
    Wire* wire{};
    DWORD explorerPid{};
    ~Owner(){if(wire) UnmapViewOfFile(wire);}
};
std::mutex gOwnerLock;
std::shared_ptr<Owner> gOwner;
HANDLE gOwnerChanged{};
std::shared_ptr<Owner> CurrentOwner(){std::scoped_lock lock(gOwnerLock);return gOwner;}
// The helper is the single writer. A bounded seqlock read keeps mode, tint and
// strength coherent without ever waiting indefinitely on Explorer's UI thread.
bool ReadOptions(const std::shared_ptr<Owner>& owner, AppearanceOptions& options) {
    if(!owner) return false;
    auto wire = owner->wire;
    for(unsigned attempt=0;attempt<3;++attempt) {
        const LONG before = InterlockedCompareExchange(&wire->optionsRevision,0,0);
        if(before & 1) continue;
        options.mode = static_cast<DWORD>(InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(&wire->options.mode),0,0));
        options.opacity = static_cast<DWORD>(InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(&wire->options.opacity),0,0));
        options.tint = static_cast<DWORD>(InterlockedCompareExchange(reinterpret_cast<volatile LONG*>(&wire->options.tint),0,0));
        options.showBorder = InterlockedCompareExchange(&wire->options.showBorder,0,0);
        if(before == InterlockedCompareExchange(&wire->optionsRevision,0,0)) return true;
    }
    return false;
}
void Report(const std::shared_ptr<Owner>& owner, DWORD state, HRESULT error, DWORD count=0) {
    if(!owner) return;
    owner->wire->snapshot.error=error;
    owner->wire->snapshot.backgrounds=count;
    owner->wire->snapshot.explorer=GetCurrentProcessId();
    // Publish state last so readers do not treat an in-progress write as ready.
    InterlockedExchange(reinterpret_cast<volatile LONG*>(&owner->wire->snapshot.state),state);
}

struct Entry {
    xaml::Shapes::Rectangle rectangle{nullptr};
    media::Brush original{nullptr};
    core::CoreDispatcher dispatcher{nullptr};
    winrt::Windows::System::DispatcherQueue queue{nullptr};
    std::atomic_bool ready{false};
    std::atomic_bool removed{false};
    int64_t fillCallback{};
    bool changing{}; // Accessed only on the element's UI thread.
    bool background{};
};
std::mutex gEntriesLock;
std::unordered_map<InstanceHandle,std::shared_ptr<Entry>> gEntries;
std::atomic<DWORD> gGeneration{0};
winrt::Windows::UI::Color TintColor(const AppearanceOptions& options) {
    auto opacity = std::min<DWORD>(options.opacity, 100);
    auto alpha = static_cast<BYTE>((opacity * 255u + 50u) / 100u);
    return winrt::Windows::UI::Color{
        alpha,
        static_cast<BYTE>((options.tint >> 16) & 0xFF),
        static_cast<BYTE>((options.tint >> 8) & 0xFF),
        static_cast<BYTE>(options.tint & 0xFF)
    };
}
media::Brush BackgroundBrush(const AppearanceOptions& options) {
    if(options.mode == kModeTransparent) {
        return media::SolidColorBrush(winrt::Windows::UI::Color{0,0,0,0});
    }
    auto color = TintColor(options);
    if(options.mode == kModeAcrylic) {
        // Alpha is controlled only by TintOpacity, not twice via TintColor.A.
        color.A = 255;
        media::AcrylicBrush acrylic;
        acrylic.BackgroundSource(media::AcrylicBackgroundSource::Backdrop);
        acrylic.TintColor(color);
        acrylic.TintOpacity(static_cast<double>(std::min<DWORD>(options.opacity, 100)) / 100.0);
        acrylic.FallbackColor(color);
        return acrylic;
    }
    return media::SolidColorBrush(color);
}
bool SameOptions(const AppearanceOptions& left, const AppearanceOptions& right) {
    return left.mode == right.mode && left.opacity == right.opacity && left.tint == right.tint && left.showBorder == right.showBorder;
}
bool IsCurrentRequest(const std::shared_ptr<Owner>& owner,bool active,const AppearanceOptions& options) {
    if(!owner || (owner->wire->enabled>0)!=active) return false;
    if(!active) return true;
    AppearanceOptions current{};
    return ReadOptions(owner,current) && SameOptions(current,options);
}
void Apply(bool active, const AppearanceOptions& options, const std::shared_ptr<Owner>& owner) {
    std::vector<std::shared_ptr<Entry>> entries;
    {std::scoped_lock lock(gEntriesLock);for(auto& [id,entry]:gEntries) if(entry->ready && !entry->removed) entries.push_back(entry);}
    if(entries.empty()) return;
    auto pending=std::make_shared<std::atomic<DWORD>>(static_cast<DWORD>(entries.size()));
    auto failed=std::make_shared<std::atomic<HRESULT>>(S_OK);
    auto count=static_cast<DWORD>(std::count_if(entries.begin(),entries.end(),[](auto& e){return e->background;}));
    if(active && count==0) return;
    auto generation=++gGeneration;
    for(auto& entry:entries) {
        auto work=[entry,active,options,owner,pending,failed,count,generation] {
            try {
                // An obsolete queued operation must never overwrite a newer restore/apply.
                if(generation==gGeneration.load() && IsCurrentRequest(owner,active,options) && !entry->removed) {
                    struct ChangeGuard { bool& value; ChangeGuard(bool& v):value(v){value=true;} ~ChangeGuard(){value=false;} } guard(entry->changing);
                    if(!active) {
                        entry->rectangle.Fill(entry->original);
                    } else if(entry->background) {
                        entry->rectangle.Fill(BackgroundBrush(options));
                    } else if(options.showBorder) {
                        entry->rectangle.Fill(entry->original);
                    } else {
                        media::SolidColorBrush clear(winrt::Windows::UI::Color{0,0,0,0});
                        entry->rectangle.Fill(clear);
                    }
                }
            } catch(...) {failed->store(winrt::to_hresult());}
            if(--*pending==0 && generation==gGeneration.load() && IsCurrentRequest(owner,active,options)) {
                auto hr=failed->load();
                Report(owner,FAILED(hr)?6:(active?2:0),hr,count);
            }
        };
        try {
            if(entry->queue) winrt::check_bool(entry->queue.TryEnqueue(work));
            else if(entry->dispatcher) entry->dispatcher.RunAsync(core::CoreDispatcherPriority::Normal,work);
            else winrt::throw_hresult(E_UNEXPECTED);
        }
        catch(...) {failed->store(winrt::to_hresult());if(--*pending==0 && generation==gGeneration.load() && IsCurrentRequest(owner,active,options)) Report(owner,6,failed->load(),count);}
    }
}

void ApplyCurrent(const std::shared_ptr<Owner>& owner) {
    AppearanceOptions options{};
    if(ReadOptions(owner,options)) Apply(true,options,owner);
}

struct TreeWatcher : winrt::implements<TreeWatcher,IVisualTreeServiceCallback2,winrt::non_agile> {
    winrt::com_ptr<IXamlDiagnostics> diagnostics;
    winrt::com_ptr<IVisualTreeService3> service;
    explicit TreeWatcher(IUnknown* site) {
        winrt::check_hresult(site->QueryInterface(diagnostics.put()));
        winrt::check_hresult(site->QueryInterface(service.put()));
    }
    HRESULT __stdcall OnVisualTreeChange(ParentChildRelation, VisualElement element, VisualMutationType mutation) noexcept override {
        try {
            if(mutation==Remove) {
                std::shared_ptr<Entry> removed;
                {std::scoped_lock lock(gEntriesLock);auto found=gEntries.find(element.Handle);if(found!=gEntries.end()){removed=found->second;gEntries.erase(found);}}
                if(removed) {
                    removed->removed=true;
                    try {removed->rectangle.UnregisterPropertyChangedCallback(xaml::Shapes::Shape::FillProperty(),removed->fillCallback);} catch(...) {}
                }
                return S_OK;
            }
            if(mutation!=Add || !element.Name) return S_OK;
            std::wstring_view name(element.Name);
            if(name!=L"BackgroundFill" && name!=L"BackgroundStroke") return S_OK;
            if(!element.Type || std::wstring_view(element.Type)!=L"Windows.UI.Xaml.Shapes.Rectangle") return S_OK;
            winrt::com_ptr<::IInspectable> inspectable;
            winrt::check_hresult(diagnostics->GetIInspectableFromHandle(element.Handle, reinterpret_cast<::IInspectable**>(inspectable.put())));
            auto rectangle=inspectable.as<xaml::Shapes::Rectangle>();
            // Names alone are not enough: reject unrelated XAML backgrounds.
            auto parent=rectangle.as<xaml::DependencyObject>();
            bool taskbar=false;
            for(unsigned depth=0;parent && depth<24;++depth) {
                if(winrt::get_class_name(parent)==L"Taskbar.TaskbarFrame") {taskbar=true;break;}
                parent=media::VisualTreeHelper::GetParent(parent);
            }
            if(!taskbar) return S_OK;
            auto entry=std::make_shared<Entry>();
            entry->rectangle=rectangle;entry->original=rectangle.Fill();
            entry->dispatcher=rectangle.Dispatcher();entry->background=name==L"BackgroundFill";
            entry->queue=winrt::Windows::System::DispatcherQueue::GetForCurrentThread();
            entry->ready=entry->original!=nullptr;
            {std::scoped_lock lock(gEntriesLock);if(!gEntries.try_emplace(element.Handle,entry).second) return S_OK;}
            std::weak_ptr<Entry> weakEntry(entry);
            entry->fillCallback=rectangle.RegisterPropertyChangedCallback(xaml::Shapes::Shape::FillProperty(),[weakEntry](auto const&, auto const&) {
                try {
                    auto current=weakEntry.lock();
                    if(!current || current->changing || current->removed) return;
                    auto updated=current->rectangle.Fill();
                    if(updated) {current->original=updated;current->ready=true;}
                    auto owner=CurrentOwner();
                    if(owner && owner->wire->enabled>0 && current->ready) ApplyCurrent(owner);
                } catch(...) {Report(CurrentOwner(),6,winrt::to_hresult());}
            });
            auto owner=CurrentOwner();
            if(owner && owner->wire->enabled>0) ApplyCurrent(owner);
            return S_OK;
        } catch(...) {auto hr=winrt::to_hresult();Report(CurrentOwner(),6,hr);return hr;}
    }
    HRESULT __stdcall OnElementStateChanged(InstanceHandle, VisualElementState, LPCWSTR) noexcept override {return S_OK;}
};
winrt::com_ptr<TreeWatcher> gWatcher;
struct TapSite : winrt::implements<TapSite,IObjectWithSite> {
    winrt::com_ptr<IUnknown> site;
    HRESULT __stdcall SetSite(IUnknown* value) noexcept override {
        try {
            if(!value) return S_OK;
            site.copy_from(value);
            if(!gWatcher) {
                gWatcher=winrt::make_self<TreeWatcher>(value);
                auto watcher=gWatcher;
                // Diagnostics can call back during subscription; keep that work outside SetSite.
                std::thread([watcher]{
                    try {
                        winrt::init_apartment(winrt::apartment_type::multi_threaded);
                        auto hr=watcher->service->AdviseVisualTreeChange(watcher.get());
                        if(FAILED(hr)) Report(CurrentOwner(),6,hr);
                    } catch(...) {Report(CurrentOwner(),6,winrt::to_hresult());}
                }).detach();
            }
            return S_OK;
        } catch(...) {auto hr=winrt::to_hresult();Report(CurrentOwner(),6,hr);return hr;}
    }
    HRESULT __stdcall GetSite(REFIID iid,void** value) noexcept override {return site?site->QueryInterface(iid,value):E_FAIL;}
};
struct Factory : winrt::implements<Factory,IClassFactory> {
    HRESULT __stdcall CreateInstance(IUnknown* outer,REFIID iid,void** value) noexcept override {
        if(outer) return CLASS_E_NOAGGREGATION;
        try {return winrt::make_self<TapSite>()->QueryInterface(iid,value);} catch(...) {return winrt::to_hresult();}
    }
    HRESULT __stdcall LockServer(BOOL) noexcept override {return S_OK;}
};
STDAPI DllGetClassObject(REFCLSID clsid,REFIID iid,void** value) {
    if(clsid!=kTapClass) return CLASS_E_CLASSNOTAVAILABLE;
    try {return winrt::make_self<Factory>()->QueryInterface(iid,value);} catch(...) {return winrt::to_hresult();}
}
STDAPI DllCanUnloadNow() {return S_FALSE;}

void InitializeDiagnostics() {
    if(gDiagnosticsStarted.exchange(true)) return;
    std::thread([]{
        try {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
            auto xamlDll=LoadLibraryExW(L"Windows.UI.Xaml.dll",nullptr,LOAD_LIBRARY_SEARCH_SYSTEM32);
            if(!xamlDll) {Report(CurrentOwner(),6,HRESULT_FROM_WIN32(GetLastError()));gDiagnosticsStarted=false;return;}
            auto initialize=reinterpret_cast<decltype(&InitializeXamlDiagnosticsEx)>(GetProcAddress(xamlDll,"InitializeXamlDiagnosticsEx"));
            std::wstring location(32768,L'\0');
            auto length=GetModuleFileNameW(gModule,location.data(),static_cast<DWORD>(location.size()));
            location.resize(length);
            HRESULT result=E_NOINTERFACE;
            if(initialize && length) {
                for(unsigned endpoint=1;endpoint<=20;++endpoint) {
                    auto owner=CurrentOwner();
                    if(!owner || owner->wire->enabled<=0) break;
                    std::thread attempt([&]{
                        auto name=L"VisualDiagConnection"+std::to_wstring(endpoint);
                        result=initialize(name.c_str(),GetCurrentProcessId(),nullptr,location.c_str(),kTapClass,nullptr);
                    });
                    attempt.join();
                    if(SUCCEEDED(result)) break;
                    Sleep(200);
                }
            }
            FreeLibrary(xamlDll);
            if(FAILED(result)) {auto owner=CurrentOwner();if(owner && owner->wire->enabled>0) Report(owner,6,result);gDiagnosticsStarted=false;}
        } catch(...) {auto owner=CurrentOwner();if(owner && owner->wire->enabled>0) Report(owner,6,winrt::to_hresult());gDiagnosticsStarted=false;}
    }).detach();
}
void MonitorOwner() {
    for(;;) {
        auto owner=CurrentOwner();
        if(!owner) {WaitForSingleObject(gOwnerChanged,INFINITE);continue;}
        LONG last=-2; auto start=GetTickCount64(); DWORD previousCount=0; AppearanceOptions previousOptions{};
        unsigned restoreRetries=0;ULONGLONG restoreAttempt=0;ULONGLONG classicPollAt=0;Snapshot classicSnapshot{};
        for(;;) {
            auto current=CurrentOwner();if(current!=owner) break;
            bool dead=WaitForSingleObject(owner->process.value,200)==WAIT_OBJECT_0;
            if(dead) InterlockedExchange(&owner->wire->enabled,-1);
            LONG desired=InterlockedCompareExchange(&owner->wire->enabled,0,0);
            DWORD count=0;{std::scoped_lock lock(gEntriesLock);for(auto& [id,e]:gEntries) if(e->background && e->ready && !e->removed) ++count;}
            AppearanceOptions options{};
            if(desired>0 && !ReadOptions(owner,options)) continue;
            if(IsClassicWindows()) {
                const auto now=GetTickCount64();
                const bool intentChanged=desired!=last;
                const bool optionsChanged=!SameOptions(previousOptions,options);
                if(intentChanged) {restoreRetries=0;restoreAttempt=now;}
                const bool retryRestore=desired<=0 && FAILED(classicSnapshot.error) && restoreRetries<4 &&
                    now-restoreAttempt>=static_cast<ULONGLONG>(500u<<restoreRetries);
                const bool topologyPoll=desired>0 && now-classicPollAt>=2000;
                if(intentChanged || retryRestore || (desired>0 && (optionsChanged || gClassicDirty.load() || topologyPoll))) {
                    if(retryRestore) {++restoreRetries;restoreAttempt=now;}
                    ClassicUpdate(desired>0,options,owner,&classicSnapshot);
                    Report(owner,classicSnapshot.state,classicSnapshot.error,classicSnapshot.backgrounds);
                    classicPollAt=now;
                }
                last=desired;previousOptions=options;
                if((dead || desired<0) && (SUCCEEDED(classicSnapshot.error) || restoreRetries>=4)) {
                    std::scoped_lock lock(gOwnerLock);if(gOwner==owner) gOwner.reset();break;
                }
                // A signaled process handle no longer supplies the regular wait.
                // Back off failed death-restoration instead of busy-spinning.
                if(dead && FAILED(classicSnapshot.error)) Sleep(200);
                continue;
            }
            const bool optionsChanged = !SameOptions(previousOptions,options);
            if(desired!=last) {restoreRetries=0;restoreAttempt=GetTickCount64();}
            if(desired<=0 && count && owner->wire->snapshot.state==3 && GetTickCount64()-restoreAttempt>5000) Report(owner,6,HRESULT_FROM_WIN32(WAIT_TIMEOUT),count);
            const bool retryRestore=desired==0 && count && owner->wire->snapshot.state==6 && restoreRetries<4 &&
                GetTickCount64()-restoreAttempt>=static_cast<ULONGLONG>(500u<<restoreRetries);
            if(retryRestore) {
                ++restoreRetries;restoreAttempt=GetTickCount64();Report(owner,3,S_OK,count);Apply(false,{},owner);
            }
            if(desired>0 && desired!=last && !gDiagnosticsStarted.load()) InitializeDiagnostics();
            if(desired!=last || count!=previousCount || optionsChanged) {
                if(count) Apply(desired>0,options,owner);
                else Report(owner,desired>0?1:0,S_OK);
                if(desired!=last || previousCount>0) start=GetTickCount64();
                last=desired;previousCount=count;previousOptions=options;
            }
            if(desired>0 && !count && GetTickCount64()-start>10000 && owner->wire->snapshot.state==1) Report(owner,6,HRESULT_FROM_WIN32(ERROR_NOT_FOUND));
            if(dead || desired<0) {
                std::scoped_lock lock(gOwnerLock);if(gOwner==owner) gOwner.reset();break;
            }
        }
    }
}
// v2 is the existing wire layout, including material options. Do not change
// the wire without a new version: a pinned resident owns the original brushes.
extern "C" __declspec(dllexport) DWORD __cdecl CWTaskbarWireVersion() noexcept {return 2;}

HRESULT AttachOwner(DWORD pid,HWND taskbar) noexcept {
    try {
        DWORD taskbarPid{};
        if(!taskbar || !GetWindowThreadProcessId(taskbar,&taskbarPid) || taskbarPid!=GetCurrentProcessId()) return E_INVALIDARG;
        auto owner=std::make_shared<Owner>();
        owner->explorerPid=taskbarPid;
        owner->mapping.value=OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,MapName(pid).c_str());
        if(owner->mapping.value) owner->wire=static_cast<Wire*>(MapViewOfFile(owner->mapping.value,FILE_MAP_ALL_ACCESS,0,0,sizeof(Wire)));
        if(!owner->wire || owner->wire->magic!=kMagic || owner->wire->owner!=pid) return E_INVALIDARG;
        owner->process.value=OpenProcess(SYNCHRONIZE,FALSE,pid);
        if(!owner->process.value) {auto hr=HRESULT_FROM_WIN32(GetLastError());Report(owner,6,hr);InterlockedExchange(&owner->wire->attached,1);return hr;}
        auto old=CurrentOwner();
        if(old && old->wire->owner!=pid && WaitForSingleObject(old->process.value,0)!=WAIT_OBJECT_0) {
            auto hr=HRESULT_FROM_WIN32(ERROR_BUSY);Report(owner,4,hr);InterlockedExchange(&owner->wire->attached,1);return hr;
        }
        // Never unload a resident with asynchronous XAML callbacks.
        HMODULE pinned{};
        if(!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_PIN,reinterpret_cast<LPCWSTR>(&AttachOwner),&pinned)) return HRESULT_FROM_WIN32(GetLastError());
        if(!SetPropW(taskbar,L"ConvenientWindow.Taskbar.Module.v1",reinterpret_cast<HANDLE>(gModule))) {
            auto hr=HRESULT_FROM_WIN32(GetLastError());Report(owner,6,hr);InterlockedExchange(&owner->wire->attached,1);return hr;
        }
        {std::scoped_lock lock(gOwnerLock);gOwner=owner;}
        if(!gOwnerChanged) {
            gOwnerChanged=CreateEventW(nullptr,FALSE,FALSE,nullptr);
            if(!gOwnerChanged) {auto hr=HRESULT_FROM_WIN32(GetLastError());Report(owner,6,hr);InterlockedExchange(&owner->wire->attached,1);return hr;}
            std::thread([]{try {MonitorOwner();} catch(...) {auto current=CurrentOwner();if(current){Apply(false,{},current);Report(current,6,winrt::to_hresult());}}}).detach();
        }
        InterlockedExchange(&owner->wire->attached,1);
        SetEvent(gOwnerChanged);
        if(owner->wire->enabled>0 && !IsClassicWindows()) InitializeDiagnostics();
        return S_OK;
    } catch(...) {return winrt::to_hresult();}
}
extern "C" __declspec(dllexport) HRESULT __cdecl CWTaskbarAttachOwner(DWORD pid,HWND taskbar) noexcept {return AttachOwner(pid,taskbar);}

bool IsClassicTaskbarWindow(HWND hwnd) {
    wchar_t name[64]{};GetClassNameW(hwnd,name,static_cast<int>(std::size(name)));
    return !wcscmp(name,L"Shell_TrayWnd") || !wcscmp(name,L"Shell_SecondaryTrayWnd");
}
bool IsClassicDirtyMessage(UINT message) {
    return message==WM_DWMCOMPOSITIONCHANGED || message==WM_SETTINGCHANGE || message==WM_THEMECHANGED
        || message==WM_DISPLAYCHANGE || message==WM_DPICHANGED;
}
struct ResidentModuleSearch { DWORD pid; HMODULE module{}; };
BOOL CALLBACK FindResidentModule(HWND hwnd,LPARAM parameter) {
    auto search=reinterpret_cast<ResidentModuleSearch*>(parameter);
    DWORD pid{};if(!GetWindowThreadProcessId(hwnd,&pid) || pid!=search->pid) return TRUE;
    wchar_t name[64]{};GetClassNameW(hwnd,name,static_cast<int>(std::size(name)));
    if(wcscmp(name,L"Shell_TrayWnd") && wcscmp(name,L"Shell_SecondaryTrayWnd")) return TRUE;
    const auto marker=GetPropW(hwnd,L"ConvenientWindow.Taskbar.Module.v1");
    if(marker) {search->module=reinterpret_cast<HMODULE>(marker);return FALSE;}
    return TRUE;
}
HMODULE ResidentModuleForTaskbar(HWND hwnd) {
    DWORD pid{};
    if(!hwnd || !GetWindowThreadProcessId(hwnd,&pid) || pid!=GetCurrentProcessId() || !IsClassicTaskbarWindow(hwnd)) return nullptr;
    if(const auto marker=GetPropW(hwnd,L"ConvenientWindow.Taskbar.Module.v1")) return reinterpret_cast<HMODULE>(marker);
    ResidentModuleSearch search{pid};EnumWindows(FindResidentModule,reinterpret_cast<LPARAM>(&search));return search.module;
}
void NotifyClassicChangeToResident(UINT message,HWND hwnd) {
    const auto resident=ResidentModuleForTaskbar(hwnd);
    if(!resident || resident==gModule) {MarkClassicChange(message,hwnd);return;}
    HMODULE verified{};
    const bool residentLoaded=GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,reinterpret_cast<LPCWSTR>(resident),&verified) && verified==resident;
    struct ModuleGuard {HMODULE module;~ModuleGuard(){if(module) FreeLibrary(module);}} residentReference{verified};
    using Version=DWORD(__cdecl*)();
    using Mark=void(__cdecl*)(UINT,HWND);
    const auto version=residentLoaded?reinterpret_cast<Version>(GetProcAddress(resident,"CWTaskbarWireVersion")):nullptr;
    const auto mark=residentLoaded && version && version()==2?reinterpret_cast<Mark>(GetProcAddress(resident,"CWTaskbarMarkClassicDirty")):nullptr;
    if(TaskbarModulePolicy::ShouldForwardClassicDirty(resident,gModule,residentLoaded,mark!=nullptr)) mark(message,hwnd);
    else if(!residentLoaded) MarkClassicChange(message,hwnd);
    // A loaded legacy resident without the new export intentionally receives no
    // local dirty bit; its old hook remains the only safe owner of its state.
}
bool IsLegacyV2Module(HMODULE module) {
    std::wstring path(32768,L'\0');
    const auto length=GetModuleFileNameW(module,path.data(),static_cast<DWORD>(path.size()));
    if(!length || length>=path.size()) return false;
    path.resize(length);
    const auto slash=path.find_last_of(L"\\/");
    const auto leaf=std::wstring_view(path).substr(slash==std::wstring::npos?0:slash+1);
    // The known v2 legacy controller materializes only this immutable filename.
    return TaskbarModulePolicy::IsLegacyV2Name(leaf) &&
        GetProcAddress(module,"CWTaskbarUpdate") && GetProcAddress(module,"CWTaskbarClose") &&
        GetProcAddress(module,"TaskbarHook") && GetProcAddress(module,"DllGetClassObject");
}
extern "C" __declspec(dllexport) LRESULT CALLBACK TaskbarHook(int code,WPARAM wparam,LPARAM lparam) noexcept {
    bool chainCalled=false;
    LRESULT chainResult{};
    if(code>=0) {
        if(!gAttachMessage) gAttachMessage=RegisterWindowMessageW(L"ConvenientWindow.Taskbar.Attach.v2");
        auto message=reinterpret_cast<CWPSTRUCT*>(lparam);
        if(message && IsClassicDirtyMessage(message->message)) NotifyClassicChangeToResident(message->message,message->hwnd);
        if(message && message->message==gAttachMessage) {
            try {
                const auto pid=static_cast<DWORD>(message->wParam);
                const auto installed=GetPropW(message->hwnd,L"ConvenientWindow.Taskbar.Module.v1");
                auto resident=reinterpret_cast<HMODULE>(installed);
                HMODULE verified{};
                const bool residentLoaded=installed && GetModuleHandleExW(
                    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
                    reinterpret_cast<LPCWSTR>(resident),&verified) && verified==resident;
                struct ModuleGuard {HMODULE module;~ModuleGuard(){if(module) FreeLibrary(module);}} residentReference{verified};
                if(TaskbarModulePolicy::HasForeignModule(installed,gModule) && residentLoaded) {
                    // Reuse the resident's watcher and saved original brushes,
                    // not merely its marker. A second watcher would capture our
                    // transparent brush as the supposed Windows default.
                    using Version=DWORD(__cdecl*)();
                    using Attach=HRESULT(__cdecl*)(DWORD,HWND);
                    const auto version=reinterpret_cast<Version>(GetProcAddress(resident,"CWTaskbarWireVersion"));
                    const auto attach=reinterpret_cast<Attach>(GetProcAddress(resident,"CWTaskbarAttachOwner"));
                    if(version && version()==2 && attach) {
                        attach(pid,message->hwnd);
                    } else if(!version && IsLegacyV2Module(resident)) {
                        // 0.6.3 has the same wire but only exports the hook.
                        // Its globals refer to the resident module, so its old
                        // version guard succeeds without injecting a watcher.
                        using Hook=LRESULT(CALLBACK*)(int,WPARAM,LPARAM);
                        auto hook=reinterpret_cast<Hook>(GetProcAddress(resident,"TaskbarHook"));
                        chainResult=hook(code,wparam,lparam);chainCalled=true;
                    } else {
                        auto owner=std::make_shared<Owner>();
                        owner->mapping.value=OpenFileMappingW(FILE_MAP_ALL_ACCESS,FALSE,MapName(pid).c_str());
                        if(owner->mapping.value) owner->wire=static_cast<Wire*>(MapViewOfFile(owner->mapping.value,FILE_MAP_ALL_ACCESS,0,0,sizeof(Wire)));
                        if(owner->wire && owner->wire->magic==kMagic && owner->wire->owner==pid) {
                            Report(owner,6,TaskbarModulePolicy::ProductVersionConflict());InterlockedExchange(&owner->wire->attached,1);
                        }
                    }
                } else {
                    // Clearing is safe only when the marker no longer identifies
                    // a loaded module. A loaded incompatible watcher must stay.
                    if(TaskbarModulePolicy::ShouldClearStaleMarker(installed,gModule,residentLoaded)) RemovePropW(message->hwnd,L"ConvenientWindow.Taskbar.Module.v1");
                    AttachOwner(pid,message->hwnd);
                }
            } catch(...) {}
        }
    }
    return chainCalled?chainResult:CallNextHookEx(nullptr,code,wparam,lparam);
}

bool IsModernWindows() {
    static const bool result=[] {
        using VersionFunction=LONG(WINAPI*)(OSVERSIONINFOW*);
        auto ntdll=GetModuleHandleW(L"ntdll.dll");
        auto versionFunction=reinterpret_cast<VersionFunction>(GetProcAddress(ntdll,"RtlGetVersion"));
        OSVERSIONINFOW version{};version.dwOSVersionInfoSize=sizeof(version);
        return versionFunction && versionFunction(&version)==0 && version.dwMajorVersion==10 && version.dwBuildNumber>=22621;
    }();
    return result;
}

// The classic backend shares the existing Explorer-side owner monitor and
// lifetime fence but never loads XAML Diagnostics. The private Accent policy
// lacks a reliable original-policy getter, so restore asks Explorer to return
// to its system default rather than claiming an exact original-policy replay.
bool IsClassicWindows() {
    static const bool result=[] {
        using VersionFunction=LONG(WINAPI*)(OSVERSIONINFOW*);
        const auto versionFunction=reinterpret_cast<VersionFunction>(GetProcAddress(GetModuleHandleW(L"ntdll.dll"),"RtlGetVersion"));
        OSVERSIONINFOW version{};version.dwOSVersionInfoSize=sizeof(version);
        return versionFunction && versionFunction(&version)==0 && TaskbarModulePolicy::IsClassicWindows10Build(version.dwMajorVersion,version.dwBuildNumber);
    }();
    return result;
}
enum { WcaAccentPolicy = 19 };
struct ClassicAccentPolicy { DWORD state; DWORD flags; DWORD gradient; DWORD animation; };
struct ClassicAttributeData { DWORD attribute; PVOID data; SIZE_T size; };
using ClassicSetAttribute = BOOL (WINAPI*)(HWND, ClassicAttributeData*);
struct ClassicTaskbarEntry { HWND hwnd{}; DWORD pid{}; bool changed{}; };
struct ClassicEnumContext { std::vector<HWND>* windows; DWORD pid; };
std::vector<ClassicTaskbarEntry> gClassicTaskbars;
AppearanceOptions gClassicOptions{};
bool gClassicSeen=false;
Snapshot gClassicResult{};
ULONGLONG gClassicTopologyCheck{};
std::atomic_bool gClassicDirty{true};
std::atomic_bool gClassicChanging{false};

#if defined(CW_TASKBAR_NATIVE_TEST)
// Only the isolated regression executable defines this seam; it never calls WCA.
ClassicSetAttribute gClassicTestSetAttribute{};
#endif
ClassicSetAttribute ClassicSetWca() {
#if defined(CW_TASKBAR_NATIVE_TEST)
    return gClassicTestSetAttribute;
#else
    static const auto function=reinterpret_cast<ClassicSetAttribute>(GetProcAddress(GetModuleHandleW(L"user32.dll"),"SetWindowCompositionAttribute"));
    return function;
#endif
}
bool IsExplorerTaskbar(HWND hwnd) {
    DWORD pid{};
    if(!hwnd || !GetWindowThreadProcessId(hwnd,&pid) || !pid) return false;
    Handle process(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,FALSE,pid));
    if(!process.value) return false;
    wchar_t image[32768]{}; DWORD length=static_cast<DWORD>(std::size(image));
    if(!QueryFullProcessImageNameW(process.value,0,image,&length)) return false;
    const auto slash=wcsrchr(image,L'\\');
    return _wcsicmp(slash ? slash+1 : image,L"explorer.exe")==0;
}
HRESULT ClassicErrorHresult() {
    const auto error=GetLastError();
    return HRESULT_FROM_WIN32(error ? error : ERROR_GEN_FAILURE);
}
bool LiveClassicEntry(const ClassicTaskbarEntry& entry,DWORD residentPid) {
    DWORD pid{};wchar_t name[64]{};
    GetClassNameW(entry.hwnd,name,static_cast<int>(std::size(name)));
    return TaskbarModulePolicy::ClassicEntryBelongsToResident(entry.pid,residentPid)
        && IsWindow(entry.hwnd) && GetWindowThreadProcessId(entry.hwnd,&pid) && pid==residentPid
        && (!wcscmp(name,L"Shell_TrayWnd") || !wcscmp(name,L"Shell_SecondaryTrayWnd"));
}
HRESULT ClassicRestore(DWORD residentPid) {
    if(residentPid!=GetCurrentProcessId()) return E_INVALIDARG;
    const auto setAttribute=ClassicSetWca();
    HRESULT failure=S_OK;
    gClassicChanging=true;
    for(auto it=gClassicTaskbars.begin();it!=gClassicTaskbars.end();) {
        if(!it->changed || !LiveClassicEntry(*it,residentPid)) {it=gClassicTaskbars.erase(it);continue;}
        ClassicAccentPolicy normal{};ClassicAttributeData data{WcaAccentPolicy,&normal,sizeof(normal)};
        DWORD_PTR response{};SetLastError(ERROR_SUCCESS);
        if(!setAttribute) {
            SetLastError(ERROR_CALL_NOT_IMPLEMENTED);
            failure=ClassicErrorHresult();++it;continue;
        }
        if(!setAttribute(it->hwnd,&data) || !SendMessageTimeoutW(it->hwnd,WM_DWMCOMPOSITIONCHANGED,1,0,SMTO_ABORTIFHUNG | SMTO_BLOCK,250,&response)) {
            failure=ClassicErrorHresult();++it;
        } else it=gClassicTaskbars.erase(it);
    }
    gClassicChanging=false;
    gClassicSeen=false;gClassicTopologyCheck=0;gClassicDirty=true;
    return failure;
}
BOOL CALLBACK EnumClassicTaskbar(HWND hwnd, LPARAM parameter) {
    DWORD pid{};auto context=reinterpret_cast<ClassicEnumContext*>(parameter);
    if(context->pid!=GetCurrentProcessId() || !GetWindowThreadProcessId(hwnd,&pid) || pid!=context->pid) return TRUE;
    wchar_t name[64]{};GetClassNameW(hwnd,name,static_cast<int>(std::size(name)));
    if(wcscmp(name,L"Shell_TrayWnd") && wcscmp(name,L"Shell_SecondaryTrayWnd")) return TRUE;
    context->windows->push_back(hwnd);return TRUE;
}
void ClassicUpdate(bool enabled,const AppearanceOptions& requested,const std::shared_ptr<Owner>& owner,Snapshot* result) {
    if(CurrentOwner()!=owner || !owner->explorerPid || owner->explorerPid!=GetCurrentProcessId()
        || !IsCurrentRequest(owner,enabled,requested)) {*result={1,S_OK,0,GetCurrentProcessId()};return;}
    if(!enabled) {
        const auto hr=ClassicRestore(owner->explorerPid);
        *result={FAILED(hr)?6u:0u,hr,static_cast<DWORD>(gClassicTaskbars.size()),GetCurrentProcessId()};return;
    }
    const auto setAttribute=ClassicSetWca();
    if(!setAttribute) {*result=gClassicResult={5,HRESULT_FROM_WIN32(ERROR_CALL_NOT_IMPLEMENTED),0,GetCurrentProcessId()};return;}
    const bool dirty=gClassicDirty.exchange(false);
    bool changed=!gClassicSeen || !SameOptions(gClassicOptions,requested) || dirty;
    gClassicOptions=requested;gClassicSeen=true;
    const auto now=GetTickCount64();
    const auto residentPid=owner->explorerPid;
    if(!gClassicTopologyCheck || now-gClassicTopologyCheck>=2000) {
        std::vector<HWND> windows;
        ClassicEnumContext context{&windows,residentPid};
        EnumWindows(EnumClassicTaskbar,reinterpret_cast<LPARAM>(&context));
        gClassicTopologyCheck=now;
        for(auto it=gClassicTaskbars.begin();it!=gClassicTaskbars.end();) {
            if(!LiveClassicEntry(*it,residentPid)) {it=gClassicTaskbars.erase(it);changed=true;}else ++it;
        }
        for(const auto hwnd:windows) {
            if(std::any_of(gClassicTaskbars.begin(),gClassicTaskbars.end(),[hwnd,residentPid](const auto& entry){return entry.hwnd==hwnd && entry.pid==residentPid;})) continue;
            ClassicTaskbarEntry entry;entry.hwnd=hwnd;entry.pid=residentPid;
            gClassicTaskbars.push_back(entry);changed=true;
        }
    }
    if(gClassicTaskbars.empty()) {*result=gClassicResult={6,HRESULT_FROM_WIN32(ERROR_NOT_FOUND),0,GetCurrentProcessId()};return;}
    if(!changed) {*result=gClassicResult;return;}
    auto color=TaskbarModulePolicy::ClassicGradientColor(requested.tint,requested.mode==kModeTransparent?0:requested.opacity);
    // Legacy acrylic needs non-zero alpha; it is a compatibility effect, not
    // an exact match for the modern XAML material. Border stays system-managed.
    if(requested.mode==kModeAcrylic && !(color & 0xFF000000u)) color |= 0x01000000u;
    // The classic backend cannot reproduce the XAML stroke rectangle reliably.
    // Keep Accent flags neutral rather than drawing an unexplained left border.
    const ClassicAccentPolicy desired{TaskbarModulePolicy::ClassicAccentStateForMode(requested.mode),0u,color,0};
    gClassicChanging=true;
    for(auto& entry:gClassicTaskbars) {
        if(!LiveClassicEntry(entry,residentPid)) continue;
        if(CurrentOwner()!=owner || !IsCurrentRequest(owner,true,requested)) {gClassicChanging=false;*result={1,S_OK,0,GetCurrentProcessId()};return;}
        auto policy=desired;ClassicAttributeData write{WcaAccentPolicy,&policy,sizeof(policy)};SetLastError(ERROR_SUCCESS);
        if(!setAttribute(entry.hwnd,&write)) {gClassicChanging=false;*result=gClassicResult={6,ClassicErrorHresult(),0,GetCurrentProcessId()};return;}
        entry.changed=true;
    }
    gClassicChanging=false;
    *result=gClassicResult={2,S_OK,static_cast<DWORD>(gClassicTaskbars.size()),GetCurrentProcessId()};
}
void MarkClassicChange(UINT message,HWND hwnd) {
    if(!IsClassicWindows() || gClassicChanging.load() || !IsClassicDirtyMessage(message)) return;
    DWORD pid{};
    if(!hwnd || !GetWindowThreadProcessId(hwnd,&pid) || pid!=GetCurrentProcessId()) return;
    wchar_t name[64]{};GetClassNameW(hwnd,name,static_cast<int>(std::size(name)));
    if(!wcscmp(name,L"Shell_TrayWnd") || !wcscmp(name,L"Shell_SecondaryTrayWnd")) gClassicDirty=true;
}
extern "C" __declspec(dllexport) void __cdecl CWTaskbarMarkClassicDirty(UINT message,HWND hwnd) noexcept {
    MarkClassicChange(message,hwnd);
}
bool HasOtherTaskbarTool() {
    Handle snapshot(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));
    if(snapshot.value==INVALID_HANDLE_VALUE) return false;
    PROCESSENTRY32W entry{};entry.dwSize=sizeof(entry);
    if(Process32FirstW(snapshot.value,&entry)) do {
        if(_wcsicmp(entry.szExeFile,L"TranslucentTB.exe")==0 || _wcsicmp(entry.szExeFile,L"windhawk.exe")==0) return true;
    } while(Process32NextW(snapshot.value,&entry));
    return false;
}
// Controller ABI, called only by the helper's dedicated worker thread.
Handle gLock,gMapping,gExplorerProcess;
Wire* gWire{};HHOOK gHook{};DWORD gExplorer{};HWND gTaskbar{};
ULONGLONG gToolCheck{};bool gOtherTool{};
void CloseController() {
    if(gWire){UnmapViewOfFile(gWire);gWire=nullptr;}
    if(gHook){UnhookWindowsHookEx(gHook);gHook=nullptr;}
    if(gMapping.value){CloseHandle(gMapping.value);gMapping.value=nullptr;}
    if(gLock.value){CloseHandle(gLock.value);gLock.value=nullptr;}
    if(gExplorerProcess.value){CloseHandle(gExplorerProcess.value);gExplorerProcess.value=nullptr;}
    gExplorer=0;gTaskbar=nullptr;
}
extern "C" __declspec(dllexport) void __cdecl CWTaskbarClose() noexcept {
    if(gWire) {
        InterlockedExchange(&gWire->enabled,-1);
        for(unsigned i=0;i<50 && gWire->snapshot.state!=0;++i) Sleep(20);
    }
    CloseController();
}
extern "C" __declspec(dllexport) void __cdecl CWTaskbarUpdate(BOOL enabled,const AppearanceOptions* requestedOptions,Snapshot* result) noexcept {
    if(!result) return;
    *result={0,S_OK,0,0};
    const AppearanceOptions requested = requestedOptions ? *requestedOptions : AppearanceOptions{kModeTransparent,0,0x233A63,1};
    try {
        if(!enabled && !gWire) return;
        if(!gAttachMessage) gAttachMessage=RegisterWindowMessageW(L"ConvenientWindow.Taskbar.Attach.v2");
        if(!gAttachMessage) winrt::throw_last_error();
        HWND taskbar=FindWindowW(L"Shell_TrayWnd",nullptr);
        DWORD pid=0;DWORD thread=taskbar?GetWindowThreadProcessId(taskbar,&pid):0;
        if(!thread){
            if(gWire && gExplorerProcess.value && WaitForSingleObject(gExplorerProcess.value,0)==WAIT_OBJECT_0) CloseController();
            if(gWire && gWire->enabled>0) {InterlockedExchange(reinterpret_cast<volatile LONG*>(&gWire->snapshot.state),3);InterlockedExchange(&gWire->enabled,0);}
            if(!enabled && gWire) *result=gWire->snapshot;
            else *result={enabled?1u:0u,S_OK,0,0};
            return;
        }
        if(gWire && (pid!=gExplorer || taskbar!=gTaskbar)) CWTaskbarClose();
        // Resume automatically after a competing tool exits. Never write over it.
        if(enabled && (!gToolCheck || GetTickCount64()-gToolCheck>=1000)) {gOtherTool=HasOtherTaskbarTool();gToolCheck=GetTickCount64();}
        if(enabled && gOtherTool) {
            if(gWire && gWire->enabled>0) InterlockedExchange(&gWire->enabled,0);
            *result={4,HRESULT_FROM_WIN32(ERROR_BUSY),0,pid};return;
        }
        if(!gWire && enabled) {
            if(!IsModernWindows() && !IsClassicWindows()) {*result={5,HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED),0,pid};return;}
            if(!IsExplorerTaskbar(taskbar)) {*result={5,HRESULT_FROM_WIN32(ERROR_NOT_SUPPORTED),0,pid};return;}
            const auto userSid=UserScope();
            TaskbarModulePolicy::UserScopedMediumSecurity security(userSid);
            if(!security.valid()) winrt::throw_last_error();
            auto lockName=L"Local\\ConvenientWindow.Taskbar.Owner.v1."+userSid;
            gLock.value=CreateMutexW(security.get(),FALSE,lockName.c_str());
            if(!gLock.value) winrt::throw_last_error();
            if(GetLastError()==ERROR_ALREADY_EXISTS) {CloseController();*result={4,HRESULT_FROM_WIN32(ERROR_BUSY),0,pid};return;}
            gExplorerProcess.value=OpenProcess(SYNCHRONIZE,FALSE,pid);
            if(!gExplorerProcess.value) winrt::throw_last_error();
            gMapping.value=CreateFileMappingW(INVALID_HANDLE_VALUE,security.get(),PAGE_READWRITE,0,sizeof(Wire),MapName(GetCurrentProcessId()).c_str());
            if(!gMapping.value) winrt::throw_last_error();
            gWire=static_cast<Wire*>(MapViewOfFile(gMapping.value,FILE_MAP_ALL_ACCESS,0,0,sizeof(Wire)));
            if(!gWire) winrt::throw_last_error();
            *gWire={kMagic,GetCurrentProcessId(),1,{1,S_OK,0,pid},requested,2,0};gExplorer=pid;gTaskbar=taskbar;
            gHook=SetWindowsHookExW(WH_CALLWNDPROC,TaskbarHook,gModule,thread);
            if(!gHook) winrt::throw_last_error();
            DWORD_PTR ignored{};
            if(!SendMessageTimeoutW(taskbar,gAttachMessage,GetCurrentProcessId(),0,SMTO_ABORTIFHUNG,1000,&ignored)) winrt::throw_last_error();
            if(gWire->attached!=1) winrt::throw_hresult(HRESULT_FROM_WIN32(ERROR_DLL_INIT_FAILED));
        }
        if(gWire) {
            if(!SameOptions(gWire->options,requested)) {
                if(enabled) InterlockedExchange(reinterpret_cast<volatile LONG*>(&gWire->snapshot.state),1);
                InterlockedIncrement(&gWire->optionsRevision);
                InterlockedExchange(reinterpret_cast<volatile LONG*>(&gWire->options.mode),static_cast<LONG>(requested.mode));
                InterlockedExchange(reinterpret_cast<volatile LONG*>(&gWire->options.opacity),static_cast<LONG>(requested.opacity));
                InterlockedExchange(reinterpret_cast<volatile LONG*>(&gWire->options.tint),static_cast<LONG>(requested.tint));
                InterlockedExchange(&gWire->options.showBorder,requested.showBorder);
                InterlockedIncrement(&gWire->optionsRevision);
            }
            const bool detached=gWire->snapshot.state==4 || gWire->snapshot.error==TaskbarModulePolicy::ProductVersionConflict();
            if(!enabled && detached) {*result={0,S_OK,0,0};CloseController();return;}
            const LONG desired=enabled?1:0;
            if(InterlockedCompareExchange(&gWire->enabled,0,0)!=desired) {
                gWire->snapshot.error=S_OK;
                InterlockedExchange(reinterpret_cast<volatile LONG*>(&gWire->snapshot.state),enabled?1:3);
                // Disable is an asynchronous restore, not a teardown. Keep the
                // original watcher for a quick subsequent enable. Close uses -1.
                InterlockedExchange(&gWire->enabled,desired);
                // Restart only the resident's owner monitor, not its watcher.
                // This also covers legacy v2 and rapid on/off that coalesces
                // back to the monitor's previous desired state before a tick.
                InterlockedExchange(&gWire->attached,0);
                DWORD_PTR ignored{};
                if(!SendMessageTimeoutW(taskbar,gAttachMessage,GetCurrentProcessId(),0,SMTO_ABORTIFHUNG,1000,&ignored)) winrt::throw_last_error();
                if(gWire->attached!=1) winrt::throw_hresult(HRESULT_FROM_WIN32(ERROR_DLL_INIT_FAILED));
            }
            *result=gWire->snapshot;
        }
    } catch(...) {auto hr=winrt::to_hresult();CWTaskbarClose();*result={6,hr,0,0};}
}
BOOL WINAPI DllMain(HINSTANCE instance,DWORD reason,LPVOID) {
    if(reason==DLL_PROCESS_ATTACH){gModule=instance;DisableThreadLibraryCalls(instance);}
    return TRUE;
}

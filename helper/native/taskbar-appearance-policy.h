#pragma once
#include <windows.h>
#include <string_view>

// Policy seams used by the real hook and native regressions. The named mutex
// and resident process handle remain the live-owner authority.
namespace TaskbarModulePolicy {
inline HRESULT ProductVersionConflict() noexcept {return HRESULT_FROM_WIN32(ERROR_PRODUCT_VERSION);}

// Keep the version gate as a small, testable seam: the classic backend is used
// for supported pre-22621 taskbar families and the XAML TAP is reserved for
// Windows 11 22H2+ where the modern taskbar is expected.
inline bool IsClassicWindows10Build(DWORD major, DWORD build) noexcept {
    return major == 10 && build >= 18362 && build < 22621;
}

// Values from ACCENT_POLICY. They are kept here so the native regression can
// verify the material mapping without loading Explorer or calling the private
// user32 export.
inline DWORD ClassicAccentStateForMode(DWORD mode) noexcept {
    switch (mode) {
    case 0: return 2; // ACCENT_ENABLE_TRANSPARENTGRADIENT
    case 1: return 4; // ACCENT_ENABLE_ACRYLICBLURBEHIND
    case 2: return 1; // ACCENT_ENABLE_GRADIENT
    default: return 0; // ACCENT_DISABLED
    }
}

inline DWORD ClassicGradientColor(DWORD rgb, DWORD opacity) noexcept {
    if (opacity > 100) opacity = 100;
    const auto alpha = (opacity * 255u + 50u) / 100u;
    return (alpha << 24) | ((rgb & 0x000000FFu) << 16)
        | (rgb & 0x0000FF00u) | ((rgb & 0x00FF0000u) >> 16);
}

inline bool HasForeignModule(HANDLE installed,HMODULE current) noexcept {
    return installed && installed!=reinterpret_cast<HANDLE>(current);
}
inline bool ShouldClearStaleMarker(HANDLE installed,HMODULE current,bool residentLoaded) noexcept {
    return HasForeignModule(installed,current) && !residentLoaded;
}
inline bool IsLegacyV2Name(std::wstring_view leaf) noexcept {
    if(leaf.size()!=28 || leaf.substr(0,8)!=L"taskbar-" || leaf.substr(24)!=L".dll") return false;
    for(auto c:leaf.substr(8,16)) if(!((c>=L'0' && c<=L'9') || (c>=L'a' && c<=L'f'))) return false;
    return true;
}
} // namespace TaskbarModulePolicy

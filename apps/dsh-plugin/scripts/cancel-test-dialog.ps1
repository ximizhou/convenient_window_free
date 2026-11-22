# Cancel only a test-owned native dialog. Never send keyboard input or activate another window.
param([Parameter(Mandatory)][int]$ServiceId)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class TestDialog {
    private delegate bool Visit(IntPtr w, IntPtr unused);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Visit cb, IntPtr data);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr w, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetClassName(IntPtr w, StringBuilder value,int length);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr w, StringBuilder value,int length);
    [DllImport("user32.dll")] private static extern bool IsWindowVisible(IntPtr w);
    [DllImport("user32.dll")] private static extern bool PostMessage(IntPtr w,uint msg,IntPtr action,IntPtr control);
    public static bool Cancel(uint pid) {
        bool cancelled=false;
        EnumWindows(delegate(IntPtr w,IntPtr unused) {
            uint owner; GetWindowThreadProcessId(w,out owner);
            if(owner!=pid || !IsWindowVisible(w)) return true;
            var value=new StringBuilder(256); GetClassName(w,value,256);
            if(value.ToString()!="#32770") return true;
            value.Clear(); GetWindowText(w,value,256);
            if(value.ToString()!="Convenient Window · DSH") return true;
            cancelled=PostMessage(w,0x111,new IntPtr(2),IntPtr.Zero); // WM_COMMAND, IDCANCEL
            return !cancelled;
        },IntPtr.Zero);
        return cancelled;
    }
}
"@
$until = [DateTime]::UtcNow.AddSeconds(18)
do {
    if ([TestDialog]::Cancel($ServiceId)) { return }
    Start-Sleep -Milliseconds 125
} while ([DateTime]::UtcNow -lt $until)
throw 'No matching confirmation dialog appeared on the test process.'

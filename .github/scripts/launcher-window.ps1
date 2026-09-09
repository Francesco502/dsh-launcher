# Read-only enumeration: .NET MainWindowHandle may select winit's unnamed event window.
if (-not ('LauncherWindowProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class LauncherWindowProbe {
    delegate bool Visitor(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool EnumWindows(Visitor visitor, IntPtr parameter);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr window, StringBuilder text, int count);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr window);
    public static int Count(int processId) { return Read(processId)[0]; }
    public static int[] Read(int processId) {
        int count = 0, dpi = 0;
        EnumWindows((window, parameter) => {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner == processId && IsWindowVisible(window)) {
                var title = new StringBuilder(256);
                GetWindowText(window, title, title.Capacity);
                if (title.ToString() == "DSH启动器") { count++; dpi = (int)GetDpiForWindow(window); }
            }
            return true;
        }, IntPtr.Zero);
        return new int[] {count, dpi};
    }
}
'@
}

param(
  [int]$ExcludeProcessId = 0
)

$source = @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class WindowScanner {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr hWnd);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int maxCount);

    public static List<object> GetWindows(int excludedPid) {
        var result = new List<object>();
        EnumWindows((hWnd, lParam) => {
            if (!IsWindowVisible(hWnd) || IsIconic(hWnd)) return true;
            uint pid;
            GetWindowThreadProcessId(hWnd, out pid);
            if (pid == excludedPid) return true;
            var title = new StringBuilder(512);
            if (GetWindowText(hWnd, title, title.Capacity) == 0) return true;
            RECT r;
            if (!GetWindowRect(hWnd, out r)) return true;
            int width = r.Right - r.Left;
            int height = r.Bottom - r.Top;
            if (width < 80 || height < 50) return true;
            result.Add(new { x = r.Left, y = r.Top, width, height, title = title.ToString() });
            return true;
        }, IntPtr.Zero);
        return result;
    }
}
"@

Add-Type -TypeDefinition $source
$OutputEncoding = [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

while ($true) {
  $windows = [WindowScanner]::GetWindows($ExcludeProcessId)
  [Console]::Out.WriteLine(($windows | ConvertTo-Json -Compress -Depth 3))
  [Console]::Out.Flush()
  Start-Sleep -Milliseconds 650
}

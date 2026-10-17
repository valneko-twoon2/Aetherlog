<#
.SYNOPSIS
  Saves the window of a running process as a PNG (used by tools\make_screenshots.mjs).

.DESCRIPTION
  Captures the visible frame only (no shadow, no mouse cursor) with PrintWindow, so the window
  does not have to be uncovered.  With -Resize the window is first moved and sized so the visible
  frame measures exactly 1282 x 852, the size of the README screenshots.

  powershell -File tools\capture_window.ps1 -ProcessId 1234 -Out shot.png -Resize
#>
param(
    [Parameter(Mandatory)][int]$ProcessId,
    [Parameter(Mandatory)][string]$Out,
    [switch]$Resize
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class CaptureWin {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hgt, uint flags);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
}
'@
[void][CaptureWin]::SetProcessDPIAware()
$h = (Get-Process -Id $ProcessId).MainWindowHandle
if ($h -eq [IntPtr]::Zero) { throw 'The process has no window.' }
[void][CaptureWin]::ShowWindow($h, 9)    # SW_RESTORE
if ($Resize) {
    # Windows 11 keeps an invisible resize border of 7 px on the left, right and bottom.
    # 0x0044 = SWP_NOZORDER | SWP_SHOWWINDOW
    [void][CaptureWin]::SetWindowPos($h, [IntPtr]::Zero, 60, 40, 1282 + 14, 852 + 7, 0x0044)
    Start-Sleep -Milliseconds 800
}
[void][CaptureWin]::SetForegroundWindow($h)

$win = New-Object CaptureWin+RECT; [void][CaptureWin]::GetWindowRect($h, [ref]$win)
$vis = New-Object CaptureWin+RECT; [void][CaptureWin]::DwmGetWindowAttribute($h, 9, [ref]$vis, 16)   # DWMWA_EXTENDED_FRAME_BOUNDS
$full = New-Object System.Drawing.Bitmap ($win.R - $win.L), ($win.B - $win.T)
$g = [System.Drawing.Graphics]::FromImage($full)
$dc = $g.GetHdc()
$ok = [CaptureWin]::PrintWindow($h, $dc, 2)   # PW_RENDERFULLCONTENT: includes the WebView2 surface
$g.ReleaseHdc($dc); $g.Dispose()
if (-not $ok) { throw 'PrintWindow failed.' }
$crop = New-Object System.Drawing.Rectangle ($vis.L - $win.L), ($vis.T - $win.T), ($vis.R - $vis.L), ($vis.B - $vis.T)
$img = $full.Clone($crop, $full.PixelFormat)
$img.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$full.Dispose(); $img.Dispose()
Write-Output "$($crop.Width)x$($crop.Height)"

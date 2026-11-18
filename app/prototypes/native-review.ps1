param([int]$ReviewProcessId, [int]$Width = 0, [int]$Height = 0, [switch]$ViewAll)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public class RerunsReviewWindow {
  public delegate bool EnumProc(IntPtr hwnd, IntPtr data);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc callback, IntPtr data);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint process);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder text, int count);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hwnd, int command);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr hwnd, int x, int y, int width, int height, bool repaint);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out Point point);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
  public struct Point { public int X; public int Y; }
}
'@
[RerunsReviewWindow]::SetProcessDPIAware() | Out-Null
$script:reviewHandle = [IntPtr]::Zero
[RerunsReviewWindow]::EnumWindows({ param($handle, $data)
  [uint32]$windowProcess = 0
  [RerunsReviewWindow]::GetWindowThreadProcessId($handle, [ref]$windowProcess) | Out-Null
  if ($windowProcess -eq $ReviewProcessId) {
    $title = [System.Text.StringBuilder]::new(256)
    [RerunsReviewWindow]::GetWindowText($handle, $title, 256) | Out-Null
    if ($title.ToString() -eq 'Reruns') { $script:reviewHandle = $handle }
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
if ($script:reviewHandle -eq [IntPtr]::Zero) { throw 'The review process has no Reruns window yet.' }
[RerunsReviewWindow]::ShowWindow($script:reviewHandle, 1) | Out-Null
[RerunsReviewWindow]::SetForegroundWindow($script:reviewHandle) | Out-Null
Start-Sleep -Milliseconds 700
$bounds = [RerunsReviewWindow+Rect]::new()
[RerunsReviewWindow]::GetWindowRect($script:reviewHandle, [ref]$bounds) | Out-Null
$client = [RerunsReviewWindow+Rect]::new()
[RerunsReviewWindow]::GetClientRect($script:reviewHandle, [ref]$client) | Out-Null
if ($Width -gt 0 -and $Height -gt 0) {
  $extraWidth = $bounds.Right - $bounds.Left - $client.Right
  $extraHeight = $bounds.Bottom - $bounds.Top - $client.Bottom
  [RerunsReviewWindow]::MoveWindow($script:reviewHandle, 40, 40, $Width + $extraWidth, $Height + $extraHeight, $true) | Out-Null
  Start-Sleep -Milliseconds 700
  [RerunsReviewWindow]::GetClientRect($script:reviewHandle, [ref]$client) | Out-Null
}
if ($ViewAll) {
  if ([RerunsReviewWindow]::GetForegroundWindow() -ne $script:reviewHandle) { throw 'Review window is not foreground; refusing input.' }
  $previousPointer = [RerunsReviewWindow+Point]::new()
  [RerunsReviewWindow]::GetCursorPos([ref]$previousPointer) | Out-Null
  $allClips = [RerunsReviewWindow+Point]::new()
  $allClips.X = 80
  $allClips.Y = 140
  [RerunsReviewWindow]::ClientToScreen($script:reviewHandle, [ref]$allClips) | Out-Null
  [RerunsReviewWindow]::SetCursorPos($allClips.X, $allClips.Y) | Out-Null
  [RerunsReviewWindow]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
  [RerunsReviewWindow]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
  [RerunsReviewWindow]::SetCursorPos($previousPointer.X, $previousPointer.Y) | Out-Null
  Start-Sleep -Milliseconds 700
}
$bitmap = [System.Drawing.Bitmap]::new($client.Right, $client.Bottom)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$dc = $graphics.GetHdc()
try { [RerunsReviewWindow]::PrintWindow($script:reviewHandle, $dc, 3) | Out-Null }
finally { $graphics.ReleaseHdc($dc) }
$destination = 'D:/reruns/app/tests/shots/final-native-window.png'
if ($Width -gt 0) { $destination = "D:/reruns/app/tests/shots/final-native-$Width.png" }
$bitmap.Save($destination, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()
Write-Output $destination

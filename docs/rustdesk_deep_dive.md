# RustDesk Deep Dive: Remote Control Architecture

A comprehensive analysis of [RustDesk](https://github.com/rustdesk/rustdesk)'s remote desktop implementation and how it compares to our **ScreenControl** agent for revamping our remote control solution.

---

## 1. High-Level Architecture

### RustDesk Module Structure

| Module | Purpose |
|--------|---------|
| `libs/scrap` | Screen capture — platform backends for X11, Wayland, DXGI, Quartz |
| `libs/enigo` | Cross-platform input injection (keyboard + mouse) |
| `libs/clipboard` | File copy/paste across platforms |
| `libs/hbb_common` | Video codec, config, TCP/UDP wrapper, protobuf, utilities |
| `src/server/video_service.rs` | Frame capture loop, encoder selection, QoS, multi-display |
| `src/server/input_service.rs` | Input dispatch, cursor tracking, modifier state, key timeout |
| `src/client.rs` | Peer connection establishment |
| `src/platform/{linux,macos,windows}.rs` | OS-specific integrations |
| `src/rendezvous_mediator.rs` | NAT traversal / relay server communication |

### ScreenControl Module Structure

| Module | Purpose |
|--------|---------|
| `sc-agent/src/screen.rs` | Screen capture — Mutter D-Bus, ScreenCaptureKit, DXGI |
| `sc-agent/src/input.rs` | Input injection — Mutter D-Bus, CoreGraphics, Win32, Enigo fallback |
| `sc-agent/src/connection.rs` | WebSocket connection handling |
| `sc-agent/src/ffmpeg.rs` | FFmpeg H264 encoding helper |
| `sc-protocol` | Protobuf message definitions |
| `sc-server` | Web server / signaling |
| `sc-relay` | TURN relay server |

---

## 2. Screen Capture — Platform by Platform

### 🐧 Linux

#### RustDesk Approach

**X11 — XCB Shared Memory** ([scrap/src/x11/capturer.rs](https://github.com/rustdesk/rustdesk/blob/master/libs/scrap/src/x11/capturer.rs))

```
libc::shmget() → libc::shmat() → xcb_shm_attach() → xcb_shm_get_image()
```

- Creates a POSIX shared memory segment (`shmget` + `shmat`)
- Attaches it to the X server via `xcb_shm_attach`
- Captures frames via `xcb_shm_get_image_unchecked` — **zero-copy** from X server
- Compares raw frame data to previous frame to skip unchanged frames (`would_block_if_equal`)
- Pixel format: BGRA from X11

**Wayland — PipeWire + GStreamer** ([scrap/src/wayland/pipewire.rs](https://github.com/rustdesk/rustdesk/blob/master/libs/scrap/src/wayland/pipewire.rs))

- Uses **D-Bus XDG Desktop Portal** to request screen sharing
- Obtains a PipeWire `fd` and stream `path` via `org.freedesktop.portal.ScreenCast`
- Supports **restore tokens** to skip the consent dialog on reconnect (KDE Plasma)
- Creates a GStreamer pipeline: `pipewiresrc → appsink` with `drop=true, max-buffers=1`
- Handles **fractional scaling** by reading actual resolution from GStreamer caps
- Per-monitor position offsets via D-Bus for multi-monitor
- `PipeWireRecorder` reads frames from `AppSink`, extracts raw pixel buffer
- Detects KDE vs GNOME for position attribute availability

**Key Details:**
- `IS_X11` detection via `hbb_common::platform::linux::is_x11_or_headless()`
- Cursor data via `XFixesGetCursorImage` (X11) — includes hotspot, dimensions, ARGB pixels
- Uses `libxdo` for some operations (window management, etc.)
- Headless mode support with virtual display creation

#### ScreenControl Approach

**GNOME Mutter D-Bus + GStreamer** ([sc-agent/src/screen.rs](file:///home/fraziersystems/Documents/projects/screencontrol/server/crates/sc-agent/src/screen.rs))

- Uses `org.gnome.Mutter.ScreenCast` D-Bus API directly (no Portal)
- No consent dialog — service-level access (runs as system service)
- GStreamer pipeline with H264 encoding: `pipewiresrc → videoconvert → x264enc → appsink`
- H264 Annex B stream parsing with AUD delimiter detection
- Integrated RemoteDesktop session for coupled screen cast + input

> [!IMPORTANT]
> **Gap:** ScreenControl only supports GNOME/Mutter. RustDesk supports GNOME, KDE, and other Wayland compositors via the XDG Desktop Portal, plus X11 via XCB. We should consider adding Portal-based capture for broader distro support (especially KDE/Sway).

---

### 🍎 macOS

#### RustDesk Approach

**CGDisplayStream** ([scrap/src/quartz/capturer.rs](https://github.com/rustdesk/rustdesk/blob/master/libs/scrap/src/quartz/capturer.rs))

```
CGDisplayStreamCreateWithDispatchQueue → FrameAvailableHandler callback
```

- Uses `CGDisplayStreamCreateWithDispatchQueue` for efficient frame capture
- Callback-based: receives `IOSurfaceRef` for each frame (GPU-backed, zero-copy)
- Supports Retina display scaling via `ENABLE_RETINA` flag
- Frame status filtering (`FrameComplete` only, ignores `Idle`/`Blank`)
- Dispatch queue for async frame delivery

**Permission Handling** ([platform/macos.rs](https://github.com/rustdesk/rustdesk/blob/master/src/platform/macos.rs)):
- `AXIsProcessTrustedWithOptions` — Accessibility permission check
- `InputMonitoringAuthStatus` — Input monitoring permission
- `IsCanScreenRecording` — Screen recording permission (macOS 10.15+)
- `CanUseNewApiForScreenCaptureCheck` — Newer API for macOS 11+
- Admin authorization via `MacCheckAdminAuthorization`
- Cursor via `CGSCurrentCursorSeed` change detection

#### ScreenControl Approach

**ScreenCaptureKit + FFmpeg** ([sc-agent/src/screen.rs#L1583](file:///home/fraziersystems/Documents/projects/screencontrol/server/crates/sc-agent/src/screen.rs#L1583))

- Primary: ScreenCaptureKit → raw BGRA → pipe to FFmpeg VideoToolbox H264
- Fallback: `CGDisplayCreateImage` polling → JPEG encoding
- H264 stream read from FFmpeg stdout

> [!NOTE]
> **Comparison:** Both use Apple-native APIs. RustDesk uses the lower-level `CGDisplayStream` for zero-copy IOSurface frames. ScreenControl uses the higher-level ScreenCaptureKit which is newer (macOS 12.3+) but provides better display/window filtering. Our FFmpeg VideoToolbox pipe approach adds process overhead that RustDesk avoids with in-process encoding.

---

### 🪟 Windows

#### RustDesk Approach

**DXGI Desktop Duplication** ([scrap/src/dxgi/mod.rs](https://github.com/rustdesk/rustdesk/blob/master/libs/scrap/src/dxgi/mod.rs))

```
IDXGIOutput::DuplicateOutput → AcquireNextFrame → MapDesktopSurface / CopyResource
```

- Creates D3D11 device and output duplication
- **Fast path**: `DesktopImageInSystemMemory == TRUE` → direct map via `MapDesktopSurface`
- **Slow path**: Copy to staging texture with `CopyResource` → `Map`
- **Rotation handling**: Uses `ID3D11VideoProcessor` for rotated displays (90°/180°/270°)
- **GDI fallback**: `create_gdi()` for cases where DXGI fails (locked desktop, UAC)
- Hardware texture output support for GPU-accelerated encoding
- Multi-adapter support via `IDXGIAdapter1::GetDesc1`

**Platform Handling** ([platform/windows.rs](https://github.com/rustdesk/rustdesk/blob/master/src/platform/windows.rs)):
- UAC elevation detection (`is_process_consent_running`)
- Desktop switching (`try_change_desktop`) for login screen / UAC
- Privacy mode via Magnification API (`win_mag`)
- Portable service for non-installed mode
- Session 0 isolation handling

#### ScreenControl Approach

**Windows Graphics Capture API** ([sc-agent/src/screen.rs#L2661](file:///home/fraziersystems/Documents/projects/screencontrol/server/crates/sc-agent/src/screen.rs#L2661))

- Uses `windows-capture` crate (`GraphicsCaptureApiHandler` trait)
- Frame arrives as `Frame` → extract raw buffer
- H264 encoding via FFmpeg external process (stdin pipe)
- JPEG fallback when FFmpeg unavailable

> [!WARNING]
> **Key Difference:** RustDesk uses DXGI Desktop Duplication (DirectX 11) which is lower-level but works on **all Windows 8+** desktops including **login screens, UAC prompts, and locked desktops** (with service-level access). ScreenControl uses the Windows Graphics Capture API (WinRT) which is higher-level but **cannot capture secure/admin desktops** and requires Windows 10 1803+. RustDesk's GDI fallback provides additional compatibility.

---

## 3. Input Injection — Platform by Platform

### 🐧 Linux

#### RustDesk: Enigo + uinput + RDP Input

**Three-tier input system:**

1. **Enigo (X11)** — Default for X11 sessions
   - `XTestFakeKeyEvent` / `XTestFakeMotionEvent` via X11 test extension
   - Direct X11 key/mouse synthesis

2. **uinput (Wayland)** — For Wayland without RDP session
   ```
   setup_uinput(minx, maxx, miny, maxy) → UInputKeyboard + UInputMouse
   ```
   - Creates virtual `/dev/uinput` devices
   - `UInputKeyboard` and `UInputMouse` implement custom input traits
   - Resolution-aware absolute positioning (minx/maxx/miny/maxy)
   - Injects directly as a virtual HID device — works on **any Wayland compositor**

3. **RDP Input (Wayland/GNOME)** — For PipeWire-based sessions
   ```
   setup_rdp_input() → RdpInputKeyboard + RdpInputMouse
   ```
   - Uses D-Bus `RemoteDesktop` session for input injection
   - Keyboard via `NotifyKeyboardKeycode`
   - Mouse via absolute positioning through stream resolution mapping

**Key Input Features:**
- `fix_key_down_timeout` loop — releases stuck keys every 10 seconds
- `KEYS_DOWN` tracking — maps all currently pressed keys with timestamps
- Modifier state management with `get_modifier_state()` checking both left+right variants
- `release_device_modifiers()` on disconnect to prevent stuck modifiers
- `clear_remapped_keycode()` for XKB remapping cleanup
- Relative mouse mode support with `RELATIVE_MOUSE_CONNS` tracking per connection
- Delta clamping: `MAX_RELATIVE_MOUSE_DELTA = 10000`

#### ScreenControl: Mutter D-Bus + Enigo Fallback

```
MutterInputInjector → D-Bus calls:
  NotifyPointerMotionAbsolute(session, stream, x, y)
  NotifyPointerButton(session, button, state)
  NotifyPointerAxis(session, dx, dy)
  NotifyKeyboardKeycode(session, keycode, state)
```

- `MonitorGeometry` handles normalized→absolute coordinate mapping
- Web keycode → evdev keycode translation table
- Modifier release on session end
- Enigo fallback available but not primary

> [!IMPORTANT]
> **Gap:** ScreenControl only injects via Mutter D-Bus — limited to GNOME. RustDesk's triple-backend (enigo/uinput/RDP) supports **every Linux desktop environment**. The uinput approach is particularly powerful as it works on KDE Plasma, Sway, wlroots-based compositors, and even headless setups.

---

### 🍎 macOS

#### RustDesk: Enigo + rdev + CGWarp

- **Mouse**: `CGWarpMouseCursorPosition` for absolute positioning, `CGAssociateMouseAndMouseCursorPosition` to decouple cursor
- **Keyboard**: `CGEventCreateKeyboardEvent` + `CGEventPost(kCGHIDEventTap)`
- **macOS-specific quirks**:
  - Main thread dispatch required: `QUEUE.exec_async(move || handle_mouse_(...))`
  - `key_sleep()` — busy-wait 12ms between keystrokes (macOS `launchctl` has erratic `thread::sleep`)
  - `enigo_ignore_flags()` for modifier flag management
  - `CGEventSourceStateID` for event source state checking

#### ScreenControl: CoreGraphics FFI

```rust
CoreGraphicsInputInjector:
  CGEventCreateMouseEvent → CGEventPost(kCGHIDEventTap)
  CGEventCreateKeyboardEvent → CGEventPost(kCGHIDEventTap)
  CGEventCreateScrollWheelEvent2
```

- Direct CoreGraphics FFI bindings
- Web keycode → macOS virtual key code mapping
- Monitor geometry for multi-display coordinate mapping

> [!TIP]
> **Assessment:** Both use the same underlying CoreGraphics APIs. RustDesk adds the `key_sleep` workaround for launchctl timing issues and main-thread dispatch via GCD queue — we should adopt both. The 12ms busy-wait for keyboard events is critical for reliability when running as a LaunchAgent.

---

### 🪟 Windows

#### RustDesk: Enigo (SendInput) + Desktop Switching

- `SendInput` API with `INPUT_MOUSE` and `INPUT_KEYBOARD`
- `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_MOVE` for absolute positioning (0–65535 range)
- `try_change_desktop()` called before every input operation — handles:
  - Login desktop (`Winlogon`)
  - UAC consent desktop (`Winlogon`)
  - Default desktop switches
- `modifier_sleep()` — 1 nanosecond yield between modifier key operations (for RDP relay)
- `ENIGO_INPUT_EXTRA_VALUE` marker to distinguish synthetic from real input

#### ScreenControl: Win32 SendInput FFI

```rust
Win32InputInjector:
  SendInput(INPUT_MOUSE with MOUSEEVENTF_ABSOLUTE)
  SendInput(INPUT_KEYBOARD)
  WHEEL_DELTA for scroll
```

- Direct `user32.dll` FFI bindings
- Normalized 0–65535 coordinate space via `to_absolute_65535`
- Web keycode → Windows virtual key code mapping

> [!WARNING]
> **Gap:** ScreenControl lacks desktop switching (`try_change_desktop`) for UAC/login screen input. This is **critical** for enterprise remote support — without it, the agent can't type passwords at login or interact with UAC prompts.

---

## 4. Video Encoding Pipeline

### RustDesk

**In-process encoding** with multiple codec support:
- **VPX** (VP8/VP9) via `libvpx` — default software codec
- **AOM** (AV1) via `libaom` — optional high-efficiency
- **Hardware codecs** (`hwcodec` feature): Platform H264/H265 via FFI
- **VRAM encoding** (`vram` feature): GPU-direct encoding (NVENC/AMF/QSV)
- Codec selection based on client capability negotiation
- `EncoderCfg` enum dispatches to correct codec backend

**QoS System** (`VideoQoS`):
- `VideoFrameController` — tracks frame send/receive per connection
- Per-connection frame acknowledgment via `FrameFetchedNotifier` channels
- Adaptive timeout waiting for client frame consumption
- Multi-display support with per-display frame tracking

### ScreenControl

**External process encoding** via FFmpeg:
- Linux: GStreamer `x264enc` inline in pipeline
- macOS: FFmpeg process with VideoToolbox (`-c:v h264_videotoolbox`)
- Windows: FFmpeg process with CPU (`-c:v libx264`)
- JPEG fallback on all platforms when FFmpeg unavailable
- H264 Annex B stream parsing with AUD detection

> [!IMPORTANT]
> **Opportunity:** RustDesk's in-process encoding eliminates IPC overhead and reduces latency. Moving to in-process encoding (via `x264` crate or `openh264` crate) would significantly improve performance, especially on Windows where the FFmpeg process pipe is a bottleneck.

---

## 5. Cursor Handling

### RustDesk

Sophisticated cursor system:
- **Cursor change detection**: `CGSCurrentCursorSeed` (macOS), `XFixesGetCursorImage` (Linux), Windows cursor handle polling
- **Cursor data caching**: `cached_cursor_data` HashMap indexed by cursor handle (u64)
- **Client-side caching**: Sends full cursor data only on first appearance, then just cursor ID
- **Compressed cursor pixels**: `hbb_common::compress::compress(&data.colors[..])`
- **Position tracking**: `LATEST_SYS_CURSOR_POS` with moved-detection (`is_moved`)
- **Peer input tracking**: `LATEST_PEER_INPUT_CURSOR` with 300ms exclusion window to avoid echo

### ScreenControl

- No custom cursor rendering — relies on OS cursor
- No cursor data transmission to viewer
- No cursor position echo prevention

> [!NOTE]
> **Gap:** Adding cursor shape transmission would significantly improve the viewer experience. RustDesk's cursor caching system (full data on first see, ID-only after) is efficient and worth adopting.

---

## 6. Key Architectural Patterns Worth Adopting

### 6.1 Service Architecture (RustDesk)

RustDesk uses a **pub/sub service model**:
```
GenericService → Subscribers (connections)
  ├── VideoService (per-display)
  ├── InputService (cursor/position/focus tracking)
  └── AudioService
```
- Services run independently, broadcast to multiple subscribers
- Each connection subscribes to relevant services
- `send_without(msg, exclude_id)` for echo prevention

### 6.2 Platform Abstraction Pattern

```rust
// RustDesk: Conditional compilation at module level
#[cfg(quartz)] pub mod quartz;
#[cfg(x11)] pub mod x11;
#[cfg(all(x11, feature = "wayland"))] pub mod wayland;
#[cfg(dxgi)] pub mod dxgi;

// Common trait: TraitCapturer
pub trait TraitCapturer {
    fn frame<'a>(&'a mut self) -> io::Result<&'a [u8]>;
    fn is_gdi(&self) -> bool;
    fn set_gdi(&mut self) -> bool;
}
```

### 6.3 Input Safety

- **Stuck key prevention**: 10-second timeout loop releases all tracked keys
- **Ctrl-C handler**: `fix_key_down_timeout_at_exit()` on signal
- **Key tracking**: `KEYS_DOWN: HashMap<KeysDown, Instant>` with expiry
- **Modifier sync**: Before mouse-down, `fix_modifiers()` syncs modifier state with remote

---

## 7. Gap Analysis & Recommendations

### High Priority

| Area | Gap | RustDesk Solution | Recommendation |
|------|-----|-------------------|----------------|
| **Linux DE support** | GNOME-only | XDG Portal + PipeWire + uinput | Add Portal-based capture for KDE/Sway/wlroots |
| **Windows desktop switch** | No UAC/login support | `try_change_desktop()` | Add `OpenDesktop`/`SwitchDesktop` Win32 calls |
| **Cursor rendering** | No cursor data | Cached cursor shape + position streaming | Add cursor data channel |
| **Stuck key prevention** | Only release on disconnect | 10s timeout loop + Ctrl-C handler | Add periodic key timeout loop |
| **In-process encoding** | FFmpeg external process | libvpx/x264/hwcodec in-process | Consider `openh264` or `x264` crate |

### Medium Priority

| Area | Gap | RustDesk Solution | Recommendation |
|------|-----|-------------------|----------------|
| **Relative mouse** | Not supported | `MOUSE_TYPE_MOVE_RELATIVE` with delta clamping | Add relative mouse mode for gaming/3D |
| **Frame dedup** | No skip on unchanged | `would_block_if_equal` raw comparison | Skip encoding when frame unchanged |
| **macOS key timing** | No `key_sleep` | 12ms busy-wait between keystrokes | Add similar workaround for LaunchAgent |
| **Multi-codec** | H264 only | VP8/VP9/AV1/H264/H265 | Start with VP8 WebRTC fallback |
| **Privacy mode** | None | Windows Magnification API + topmost window | Consider for enterprise features |

### Lower Priority

| Area | Gap | RustDesk Solution | Recommendation |
|------|-----|-------------------|----------------|
| **Clipboard** | Basic text | Full file copy/paste cross-platform | Extend clipboard to file transfer |
| **Audio** | None implemented | PulseAudio capture on Linux | Future phase |
| **Headless** | No virtual display | Xvfb/virtual display creation | Future phase |
| **NAT traversal** | Relay-only | TCP hole punching + relay fallback | Current relay approach is fine |

---

## 8. Platform-Specific Takeaways

### Linux: What RustDesk Does Better
1. **XDG Desktop Portal** instead of GNOME-specific D-Bus — universal Wayland support
2. **uinput** virtual device for input injection — works everywhere, no compositor dependency
3. **X11 XCB SHM** for zero-copy capture — much faster than screenshot-based approaches
4. **Restore tokens** for PipeWire sessions — skip consent dialog on reconnect (KDE)
5. **`sudo -E` environment preservation check** — handles Ubuntu 25.10 breaking changes

### macOS: What RustDesk Does Better
1. **CGDisplayStream** with IOSurface — lower overhead than ScreenCaptureKit pipe
2. **GCD main-thread dispatch** for input — prevents crashes from background thread input
3. **Permission checking** with API version detection — graceful degradation
4. **Cursor seed monitoring** — efficient change detection without polling
5. **Busy-wait `key_sleep`** — workaround for `launchctl` timing bugs

### Windows: What RustDesk Does Better
1. **DXGI Desktop Duplication** — works on login/UAC desktops, Windows 8+
2. **Desktop switching** — seamlessly handles Winlogon/Default desktop transitions
3. **GDI fallback** — captures when DXGI fails (older GPUs, some RDP)
4. **Privacy mode** via Magnification API — blacks out physical screen during remote
5. **Display rotation** handling via D3D11 Video Processor
6. **ENIGO_INPUT_EXTRA_VALUE** — marks synthetic input to avoid self-detection loops

### Our Advantages Over RustDesk
1. **WebSocket-based** — works through any firewall/proxy, no custom UDP protocol
2. **Web viewer** — no client installation needed (React/TypeScript)
3. **Mutter D-Bus** — no consent dialog on GNOME (service-level access)
4. **ScreenCaptureKit** — newer Apple API with better window filtering
5. **Centralized management** — server/agent/viewer architecture for enterprise

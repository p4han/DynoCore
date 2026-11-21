# Dyno (DynoCore) 🚀

> **High-Performance Windows Memory Cleaner & System Optimizer**  
> *Full name: DynoCore — engineered to tune, clean, and extract peak performance from your hardware.*  
> Pure Rust · Native Win32 API · Zero Electron / Zero Web Bloat · Single Portable Binary (~450 KB)

---

## ⚡ Highlights

- **Dynamic Auto-RAM Cleaner**: Continuously monitors physical memory in the background and automatically cleans working sets & standby lists whenever RAM usage hits **65%**.
- **Always-Alive Background Operation**: Minimizes seamlessly to the Windows System Tray. Closing the window keeps Dyno running safely in the background.
- **Silent Safe Optimizations on Logon**: Applies safe, non-destructive tweaks automatically upon system startup via an elevated scheduled task.
- **Zero UAC Prompt & Reliable Autostart**: Runs with `asInvoker` manifest and registers with Windows Task Scheduler (`HighestAvailable`), eliminating annoying UAC prompts on every launch and ensuring 100% reliable autostart when your PC turns on.
- **Automatic In-App Updates**: Seamlessly checks GitHub releases in the background, displays an integrated Updating screen with real-time download progress, and automatically applies updates and restarts.
- **43 Granular System Tweaks**:
  - **Telemetry & Tracking**: Telemetry data collection, DiagTrack diagnostics, Feedback Hub, Customer Experience Improvement Program (CEIP).
  - **Gaming & Input**: Game DVR background recording, Game Bar presence, Pointer precision / mouse acceleration, Nagle's algorithm (TCP latency optimization).
  - **Visual & UI**: Window animation speeds, Menu drop-down delays, Transparency effects.
  - **Power & Hibernation**: Fast Startup cache clearance, Sleep hibernation file tuning, Ultimate Performance power plan.
  - **Windows Services**: Connected User Experiences, Xbox background services, Remote Registry, Windows Error Reporting telemetry.
- **Confirmation Safety Guard**: Displays an explicit review confirmation dialog before applying system-level tweaks.
- **Dark Windows 11 Fluent Aesthetic**: Built purely with native Win32 GDI & DirectComposition / DWM titlebar styling.

---

## 🛠️ Architecture

Dyno is engineered to be as lightweight and responsive as possible:

- **Language**: 100% Rust (2021 edition).
- **GUI Engine**: Direct Win32 GDI with custom double-buffered software rendering (`WM_PAINT`, `CreateCompatibleDC`, `BitBlt`).
- **Memory API**: Directly invokes Windows NT kernel memory list flushing (`NtSetSystemInformation` with `SystemMemoryListInformation`).
- **Binary Footprint**: ~480 KB self-contained `.exe` with embedded icon, native WinINet updater, and execution manifest (`asInvoker`).

---

## 📦 Download & Installation

1. Grab the latest `dyno.exe` or `Dyno-Setup-x64.exe` from the [Releases](https://github.com/p4han/DynoCore/releases) section.
2. Place `dyno.exe` anywhere (or run setup to install to Program Files).
3. On first launch, Dyno automatically establishes its elevated logon task. From then on, it starts automatically on PC boot with zero UAC prompts!

---

## 💻 Command Line Interface

While Dyno is designed for full GUI and automated background operation, it also provides command-line flags for scripting:

```cmd
:: Immediate 1-click RAM purge directly from terminal
dyno.exe --clean

:: Launch directly into the system tray (silent background monitoring)
dyno.exe --tray

:: Enable auto-start on Windows logon
dyno.exe --autostart-on

:: Disable auto-start
dyno.exe --autostart-off

:: Apply safe optimizations silently and exit
dyno.exe --apply-safe-only
```

---

## 🔧 Building from Source

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (1.75+ recommended)
- Windows 10 / 11 (64-bit)

```cmd
git clone https://github.com/<your-username>/dyno.git
cd dyno
cargo build --release
```

The optimized output binary will be located at `target\release\dyno.exe`.

---

## 📜 License
Licensed under the [MIT License](LICENSE).

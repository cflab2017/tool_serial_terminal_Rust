<h1 align="center">CNTerminal</h1>

<p align="center">
  🌐 <b>Live: <a href="https://www.coding-now.com/en/cnterminal">www.coding-now.com/en/cnterminal</a></b> · 🇰🇷 <a href="README.ko.md">한국어 README</a>
</p>

<p align="center">
  Portable serial terminal · single Windows .exe · no install<br/>
  Rust + <a href="https://github.com/emilk/egui">egui</a> · dark amber CRT theme
</p>

<p align="center">
  <img src="docs/CNTerminal.png" alt="CNTerminal screenshot" width="900">
</p>

---

## Download

| Version | Released | Binary | Source |
|:---:|:---:|:---|:---|
| **v0.1.0** | 2026-05-26 | [📥 CNTerminal_v0.1.0.exe](https://github.com/cflab2017/tool_serial_terminal_Rust/releases/download/v0.1.0/CNTerminal_v0.1.0.exe) | [Source (zip)](https://github.com/cflab2017/tool_serial_terminal_Rust/archive/refs/tags/v0.1.0.zip) · [Source (tar.gz)](https://github.com/cflab2017/tool_serial_terminal_Rust/archive/refs/tags/v0.1.0.tar.gz) |

> **Windows 10 / 11.** No extra runtime (.NET / WebView2 / Python) **required**.
> Just double-click the downloaded `.exe` and it runs.
> On first run, if SmartScreen warns you, click `More info` → `Run anyway` (the .exe is unsigned).
> USB-serial devices need their chipset driver (CH340 / FTDI / CP210x, etc.) installed for the port to appear.

---

## Features

### Serial connection
- Automatic port discovery + refresh
- **Arbitrary baud rate input** + 8 common presets in a dropdown
- Non-standard baud rates you type in are saved → restored in the dropdown next launch
- **DTR toggle** (avoid ESP32/Arduino auto-reset on connect, modem flow control, …)

### Console
- RX (green ◄) / TX (amber ►) / SYS / ERR color coding + timestamps (HH:MM:SS.mmm)
- **ASCII / HEX display toggle** (raw bytes are kept per line → existing log re-renders instantly on toggle)
- Zero line spacing · font size combo (9–24 px) → maximize lines per screen
- **Memory guard**: auto-trim at a 5,000-line cap
- Auto-scroll (forced toggle, or follow only when already at the bottom)
- **RX SPLIT** — for HEX dumps. If no new byte arrives within the given ms, start a new line automatically (0 = off)
- Save Log — dump the console to a .txt (file dialog)
- Clear — wipe the console instantly

### Transmit
- Multiline input (editable even before a port is connected)
- **Enter** = send / **Shift+Enter** = newline
- ENDING selector (None / LF / CR / CRLF) — applied in both TEXT and HEX modes
- **TEXT mode** — send exactly what you typed + the chosen ENDING
- **HEX mode** — free-form parsing: `AA 55 01 00 FF`, `aa,55,01`, `0xAA 0x55`, `AA55 0100FF`, … → raw bytes on the wire
- **WILL SEND preview** — always shows the exact bytes that will hit the wire, as both **ASCII (escaped) and HEX**
- Input box is preserved after sending (easy repeat-send), manual clear with `×`
- **Send history** — auto-saved (up to 50): click = load / double-click = send immediately / per-item `×` = remove

### ASCII ↔ HEX converter (popup)
- Type in either box and the other side converts live
- The ASCII side supports escape syntax: `\n \r \t \0 \\ \"  \xNN`
- Non-printable bytes are shown as `\xNN` → **round-trip accurate**
- Copy ASCII / Copy HEX buttons
- `→ Send box (TEXT / HEX)` — load the result into the send box and switch mode automatically

### Settings persistence
- Everything is auto-saved to **`cnterminal.cfg`** next to the `.exe` (human-readable key=value text)
- Restored on next launch — port/baud/display mode/toggles/font size/history/custom bauds
- An old `serial_terminal_bauds.txt`, if present, is migrated automatically

---

## Shortcuts

| Shortcut | Action |
|--------|------|
| `Enter` (send box) | Send immediately |
| `Shift+Enter` | Newline |

---

## Building (for developers)

### Requirements
- Rust 1.92+ (per eframe 0.34)
- Windows 10 / 11 (officially supported). Linux / macOS also compile

### Regular build
```bash
cargo build --release
# → target/release/cnterminal.exe
```

### Version-stamped build (for release)
```powershell
.\scripts\build.ps1
# → target/release/cnterminal.exe
# → target/release/CNTerminal_v0.1.0.exe   (version auto-extracted from Cargo.toml)
```

### Bumping the version
1. Edit `[package].version` in `Cargo.toml` only
2. Run `.\scripts\build.ps1`
3. The window title bar and the build filename pick up the version automatically

---

## Tech summary

| Area | Choice / why |
|------|-------------|
| GUI | **eframe + egui** — everything statically linked into a single .exe. No WebView2/.NET/runtime at all |
| Serial | **serialport** crate (cross-platform) |
| Threading | UI ↔ worker split over two `mpsc` channels. Worker does 50 ms timeout reads + 20 ms batching |
| Fonts | System Korean (Malgun Gothic) + Segoe UI Symbol registered as runtime fallbacks |
| Icon | Procedurally generated RGBA → window icon + multi-size ICO embedded as a PE resource |
| Config | `cnterminal.cfg` next to the exe (key=value text), diffed every frame and written only on change |

---

## License / author

- **Joseph.han** · [coding-now.com](https://coding-now.com)
- Issues / pull requests welcome.

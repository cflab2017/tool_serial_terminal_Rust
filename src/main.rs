#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Local;
use eframe::egui;
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, RichText, ScrollArea, TextStyle};

mod icon;

// ---------- Theme colors (Dark amber CRT) ----------
const BG: Color32 = Color32::from_rgb(0x0b, 0x0a, 0x08);
const PANEL: Color32 = Color32::from_rgb(0x14, 0x12, 0x10);
const PANEL2: Color32 = Color32::from_rgb(0x1c, 0x19, 0x16);
const LINE: Color32 = Color32::from_rgb(0x2a, 0x25, 0x1e);
const TEXT: Color32 = Color32::from_rgb(0xf3, 0xe7, 0xcf);
const MUTED: Color32 = Color32::from_rgb(0x7a, 0x6f, 0x5c);
const AMBER: Color32 = Color32::from_rgb(0xff, 0xb0, 0x00);
const AMBER_DIM: Color32 = Color32::from_rgb(0xb8, 0x80, 0x1f);
const GREEN: Color32 = Color32::from_rgb(0x5d, 0xdc, 0x7a);
const RED: Color32 = Color32::from_rgb(0xe0, 0x87, 0x6a);
const RX: Color32 = Color32::from_rgb(0x9a, 0xd8, 0xb0);
const TX: Color32 = Color32::from_rgb(0xe8, 0xc9, 0x8a);

const MAX_LINES: usize = 5000;
const HISTORY_MAX: usize = 50;
const BAUDS: &[u32] = &[9600, 19200, 38400, 57600, 115200, 230400, 460800, 921600];

const FONT_MIN: f32 = 9.0;
const FONT_MAX: f32 = 24.0;
const FONT_SIZES: &[i32] = &[9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 22, 24];

// Pulled from Cargo.toml at compile time so the on-screen version stays
// in sync with the package metadata.
const APP_NAME: &str = "CNTerminal";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

// ---------- Worker <-> UI messages ----------
enum UiCmd {
    Send(Vec<u8>),
    SetDtr(bool),
    Disconnect,
}

enum WorkerEvent {
    Connected,
    Disconnected,
    Data(Vec<u8>),
    Error(String),
}

// ---------- Console line model ----------
#[derive(Clone, Copy, PartialEq)]
enum LineKind {
    Rx,
    Tx,
    Sys,
    Err,
}

struct ConsoleLine {
    kind: LineKind,
    ts: String,
    // For Rx/Tx we keep the raw bytes so we can re-render in ASCII or HEX on toggle.
    // For Sys/Err the message lives in `text` and `raw` is empty.
    raw: Vec<u8>,
    text: String,
}

#[derive(Clone, Copy, PartialEq)]
enum DisplayMode {
    Ascii,
    Hex,
}

#[derive(Clone, Copy, PartialEq)]
enum SendMode {
    Text,
    Hex,
}

#[derive(Clone, Copy, PartialEq)]
enum LineEnding {
    None,
    Lf,
    Cr,
    CrLf,
}

impl LineEnding {
    fn bytes(self) -> &'static [u8] {
        match self {
            LineEnding::None => b"",
            LineEnding::Lf => b"\n",
            LineEnding::Cr => b"\r",
            LineEnding::CrLf => b"\r\n",
        }
    }
    fn label(self) -> &'static str {
        match self {
            LineEnding::None => "None",
            LineEnding::Lf => "LF (\\n)",
            LineEnding::Cr => "CR (\\r)",
            LineEnding::CrLf => "CRLF",
        }
    }
}

struct SendPreview {
    ascii: String,
    hex: String,
    bytes: usize,
    error: Option<String>,
}

// ---------- App ----------
struct SerialApp {
    // Port settings
    ports: Vec<String>,
    selected_port: String,
    baud_text: String,
    custom_bauds: Vec<u32>, // User-entered bauds (not in BAUDS), persisted next to the exe

    // Connection state
    connected: bool,
    cmd_tx: Option<Sender<UiCmd>>,
    event_rx: Option<Receiver<WorkerEvent>>,
    conn_info: String,

    // Console
    lines: VecDeque<ConsoleLine>,
    rx_partial: Vec<u8>, // bytes after last newline still being accumulated
    display_mode: DisplayMode,
    show_ts: bool,
    auto_scroll: bool,
    console_font_size: f32,

    // RX idle-split: if the line has gone this many ms without new bytes,
    // emit whatever's buffered as its own line. 0 disables the feature.
    rx_idle_ms: u32,
    rx_idle_ms_text: String,
    last_rx_at: Option<Instant>,

    // Send
    send_text: String,
    line_ending: LineEnding,
    send_mode: SendMode,
    history: Vec<String>,

    // Control signals
    dtr_on: bool,

    // Stats
    rx_bytes: u64,
    tx_bytes: u64,
    line_count: u64,

    // Last serialized config that was written to disk. Used to detect changes
    // without forcing every UI mutation to know about persistence.
    last_saved_cfg: String,

    // ASCII <-> HEX converter popup
    converter_open: bool,
    conv_ascii: String,
    conv_hex: String,
}

impl SerialApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);

        let cfg = load_config();

        let mut app = Self {
            ports: Vec::new(),
            selected_port: cfg.port.unwrap_or_default(),
            baud_text: cfg.baud.unwrap_or_else(|| "115200".to_string()),
            custom_bauds: cfg.custom_bauds,
            connected: false,
            cmd_tx: None,
            event_rx: None,
            conn_info: "Disconnected".into(),
            lines: VecDeque::with_capacity(MAX_LINES + 16),
            rx_partial: Vec::new(),
            display_mode: cfg.display_mode.unwrap_or(DisplayMode::Ascii),
            show_ts: cfg.show_ts.unwrap_or(true),
            auto_scroll: cfg.auto_scroll.unwrap_or(true),
            console_font_size: cfg
                .font_size
                .map(|v| v.clamp(FONT_MIN, FONT_MAX))
                .unwrap_or(13.0),
            rx_idle_ms: cfg.rx_idle_ms.unwrap_or(0),
            rx_idle_ms_text: cfg
                .rx_idle_ms
                .map(|v| v.to_string())
                .unwrap_or_else(|| "0".to_string()),
            last_rx_at: None,
            send_text: String::new(),
            line_ending: cfg.line_ending.unwrap_or(LineEnding::CrLf),
            send_mode: cfg.send_mode.unwrap_or(SendMode::Text),
            history: cfg.history,
            dtr_on: cfg.dtr.unwrap_or(true),
            rx_bytes: 0,
            tx_bytes: 0,
            line_count: 0,
            last_saved_cfg: String::new(),
            converter_open: false,
            conv_ascii: String::new(),
            conv_hex: String::new(),
        };
        app.refresh_ports();
        // Seed `last_saved_cfg` from what's actually on disk so we don't
        // immediately overwrite an unchanged file on first frame.
        if let Some(path) = config_path() {
            app.last_saved_cfg = std::fs::read_to_string(&path).unwrap_or_default();
        }
        app.push_sys("Ready. Select a port and click [Connect].");
        app
    }

    // ---------- Ports ----------
    fn refresh_ports(&mut self) {
        match serialport::available_ports() {
            Ok(list) => {
                self.ports = list.into_iter().map(|p| p.port_name).collect();
                self.ports.sort();
                if !self.ports.iter().any(|p| p == &self.selected_port) {
                    self.selected_port = self.ports.first().cloned().unwrap_or_default();
                }
            }
            Err(e) => {
                self.push_err(format!("Failed to list ports: {e}"));
            }
        }
    }

    // ---------- Connect / Disconnect ----------
    fn connect(&mut self, ctx: &egui::Context) {
        if self.selected_port.is_empty() {
            self.push_err("Select a port first.");
            return;
        }
        let baud = match self.baud_text.trim().parse::<u32>() {
            Ok(0) => {
                self.push_err("Baud rate must be 1 or higher.");
                return;
            }
            Ok(v) => v,
            Err(_) => {
                self.push_err(format!("Baud rate is not a number: '{}'", self.baud_text));
                return;
            }
        };

        // Remember user-entered bauds that aren't already in the preset list.
        // Persistence happens in the autosave pass at the end of `ui`.
        if !BAUDS.contains(&baud) && !self.custom_bauds.contains(&baud) {
            self.custom_bauds.push(baud);
            self.custom_bauds.sort_unstable();
        }

        let path = self.selected_port.clone();

        let (cmd_tx, cmd_rx) = channel::<UiCmd>();
        let (evt_tx, evt_rx) = channel::<WorkerEvent>();
        let ctx_clone = ctx.clone();

        let path_for_thread = path.clone();
        thread::Builder::new()
            .name(format!("serial-{path}"))
            .spawn(move || {
                serial_worker(path_for_thread, baud, cmd_rx, evt_tx, ctx_clone);
            })
            .expect("failed to spawn serial worker");

        // Apply current DTR state on connect so the worker matches the UI toggle.
        let _ = cmd_tx.send(UiCmd::SetDtr(self.dtr_on));

        self.cmd_tx = Some(cmd_tx);
        self.event_rx = Some(evt_rx);
        self.conn_info = format!("{} @ {} baud", self.selected_port, baud);
    }

    fn set_dtr(&mut self, level: bool) {
        self.dtr_on = level;
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(UiCmd::SetDtr(level));
        }
    }

    fn disconnect(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(UiCmd::Disconnect);
        }
        // event_rx kept so we can drain the final Disconnected event
    }

    // ---------- Console push ----------
    fn push_line(&mut self, line: ConsoleLine) {
        if matches!(line.kind, LineKind::Rx | LineKind::Tx) {
            self.line_count += 1;
        }
        self.lines.push_back(line);
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }
    fn push_rx_line(&mut self, raw: Vec<u8>) {
        let text = ascii_from_bytes(&raw);
        self.push_line(ConsoleLine { kind: LineKind::Rx, ts: timestamp(), raw, text });
    }
    fn push_tx_line(&mut self, raw: Vec<u8>) {
        let text = ascii_from_bytes(&raw);
        self.push_line(ConsoleLine { kind: LineKind::Tx, ts: timestamp(), raw, text });
    }
    fn push_sys(&mut self, msg: impl Into<String>) {
        self.push_line(ConsoleLine {
            kind: LineKind::Sys,
            ts: timestamp(),
            raw: Vec::new(),
            text: msg.into(),
        });
    }
    fn push_err(&mut self, msg: impl Into<String>) {
        self.push_line(ConsoleLine {
            kind: LineKind::Err,
            ts: timestamp(),
            raw: Vec::new(),
            text: msg.into(),
        });
    }

    // ---------- Incoming bytes -> line accumulator ----------
    fn ingest_rx(&mut self, mut bytes: Vec<u8>) {
        self.rx_bytes += bytes.len() as u64;
        self.rx_partial.append(&mut bytes);
        loop {
            if let Some(pos) = self.rx_partial.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.rx_partial.drain(..=pos).collect();
                line.pop(); // remove '\n'
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.push_rx_line(line);
            } else {
                break;
            }
        }
        // Flush huge partial buffers so the user sees data even without newlines.
        if self.rx_partial.len() > 4096 {
            let flush = std::mem::take(&mut self.rx_partial);
            self.push_rx_line(flush);
        }
        self.last_rx_at = Some(Instant::now());
    }

    /// Force-emit the in-progress RX line if it has been quiet for at least
    /// `rx_idle_ms` and the feature is enabled. Useful for HEX dumps where
    /// there are no natural \n separators.
    fn maybe_idle_split(&mut self) {
        if self.rx_idle_ms == 0 {
            return;
        }
        let Some(t) = self.last_rx_at else { return };
        if t.elapsed() < Duration::from_millis(self.rx_idle_ms as u64) {
            return;
        }
        if !self.rx_partial.is_empty() {
            let flush = std::mem::take(&mut self.rx_partial);
            self.push_rx_line(flush);
        }
        // Reset so we don't keep re-firing every frame.
        self.last_rx_at = None;
    }

    // ---------- Pump worker events ----------
    fn pump_events(&mut self) {
        // Drain first so we don't hold an immutable borrow of self while
        // mutating console state. Stop draining when the channel closes.
        let mut events: Vec<WorkerEvent> = Vec::new();
        let mut channel_closed = false;
        if let Some(rx) = &self.event_rx {
            loop {
                match rx.try_recv() {
                    Ok(ev) => events.push(ev),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        channel_closed = true;
                        break;
                    }
                }
            }
        }

        let mut drop_rx = false;
        for ev in events {
            match ev {
                WorkerEvent::Connected => {
                    self.connected = true;
                    self.push_sys(format!("── Connected: {} ──", self.conn_info));
                }
                WorkerEvent::Disconnected => {
                    self.connected = false;
                    if !self.rx_partial.is_empty() {
                        let flush = std::mem::take(&mut self.rx_partial);
                        self.push_rx_line(flush);
                    }
                    self.push_sys("── Disconnected ──");
                    self.conn_info = "Disconnected".into();
                    self.cmd_tx = None;
                    drop_rx = true;
                }
                WorkerEvent::Data(bytes) => {
                    self.ingest_rx(bytes);
                }
                WorkerEvent::Error(e) => {
                    self.push_err(e);
                }
            }
        }
        if channel_closed {
            if self.connected {
                self.connected = false;
                self.push_err("Worker thread terminated.");
                self.conn_info = "Disconnected".into();
            }
            self.cmd_tx = None;
            drop_rx = true;
        }
        if drop_rx {
            self.event_rx = None;
        }
    }

    // ---------- Send ----------
    fn send_now(&mut self) {
        if !self.connected {
            return;
        }

        // Build the wire payload and the bytes we want to display in the TX
        // log. For text mode these differ (log shows what the user typed,
        // wire adds ENDING); for hex mode they're identical.
        let (payload, log_bytes): (Vec<u8>, Vec<u8>) = match self.send_mode {
            SendMode::Text => {
                let raw = self.send_text.trim_end_matches(['\r', '\n']).to_string();
                if raw.is_empty() && self.line_ending == LineEnding::None {
                    return;
                }
                let mut p = raw.as_bytes().to_vec();
                p.extend_from_slice(self.line_ending.bytes());
                (p, raw.into_bytes())
            }
            SendMode::Hex => match parse_hex_bytes(&self.send_text) {
                Ok(b) if b.is_empty() && self.line_ending == LineEnding::None => {
                    return;
                }
                Ok(b) => {
                    let mut payload = b;
                    payload.extend_from_slice(self.line_ending.bytes());
                    let log = payload.clone();
                    (payload, log)
                }
                Err(e) => {
                    self.push_err(format!("Hex parse error: {e}"));
                    return;
                }
            },
        };

        if let Some(tx) = &self.cmd_tx {
            if tx.send(UiCmd::Send(payload.clone())).is_err() {
                self.push_err("Send failed: worker disconnected");
                return;
            }
        } else {
            self.push_err("Send failed: not connected");
            return;
        }
        self.tx_bytes += payload.len() as u64;
        self.push_tx_line(log_bytes);

        let h = self.send_text.trim().to_string();
        if !h.is_empty() {
            // Move-to-front: drop any earlier duplicate so the same command
            // doesn't pile up in the history list.
            self.history.retain(|x| x != &h);
            self.history.insert(0, h);
            if self.history.len() > HISTORY_MAX {
                self.history.truncate(HISTORY_MAX);
            }
        }
        // Intentionally do NOT clear `send_text` here — keeping it makes
        // repeat-sending the same payload (press Enter again) trivial. The
        // user can clear with the `×` button next to the SEND header or by
        // clicking a history entry to replace the contents.
    }

    // ---------- Save / Clear ----------
    fn save_log(&mut self) {
        let mut out = String::new();
        for ln in &self.lines {
            let arrow = match ln.kind {
                LineKind::Rx => "<",
                LineKind::Tx => ">",
                LineKind::Sys => "*",
                LineKind::Err => "x",
            };
            let body = match ln.kind {
                LineKind::Rx | LineKind::Tx => match self.display_mode {
                    DisplayMode::Ascii => ascii_from_bytes(&ln.raw),
                    DisplayMode::Hex => hex_from_bytes(&ln.raw),
                },
                _ => ln.text.clone(),
            };
            out.push_str(&format!("{} {} {}\n", ln.ts, arrow, body));
        }
        if out.is_empty() {
            self.push_err("Nothing to save.");
            return;
        }
        let default_name = format!("serial-log-{}.txt", Local::now().format("%Y%m%d-%H%M%S"));
        let chosen = rfd::FileDialog::new()
            .set_title("Save Log")
            .set_file_name(&default_name)
            .add_filter("Text", &["txt", "log"])
            .save_file();
        if let Some(path) = chosen {
            match std::fs::write(&path, out) {
                Ok(_) => self.push_sys(format!("Saved: {}", path.display())),
                Err(e) => self.push_err(format!("Save failed: {e}")),
            }
        }
    }

    fn clear_console(&mut self) {
        self.lines.clear();
        self.line_count = 0;
    }

    /// Compute the "will send" preview shown above the Send button.
    /// Always populates both the ASCII (escape-encoded) and HEX views so the
    /// user can see exactly what goes on the wire in either representation.
    fn send_preview(&self) -> SendPreview {
        const MAX: usize = 96;

        let bytes_result: Result<Vec<u8>, String> = match self.send_mode {
            SendMode::Text => {
                let raw = self.send_text.trim_end_matches(['\r', '\n']);
                let mut bytes: Vec<u8> = raw.as_bytes().to_vec();
                bytes.extend_from_slice(self.line_ending.bytes());
                Ok(bytes)
            }
            SendMode::Hex => {
                let parsed = if self.send_text.trim().is_empty() {
                    Ok(Vec::new())
                } else {
                    parse_hex_bytes(&self.send_text)
                };
                parsed.map(|mut b| {
                    b.extend_from_slice(self.line_ending.bytes());
                    b
                })
            }
        };

        match bytes_result {
            Err(e) => SendPreview {
                ascii: String::new(),
                hex: String::new(),
                bytes: 0,
                error: Some(e),
            },
            Ok(b) => {
                let (ascii, _) =
                    truncate_for_preview(&escape_preview_bytes(&b), MAX);
                let (hex, _) = truncate_for_preview(&hex_from_bytes(&b), MAX);
                SendPreview {
                    ascii: if b.is_empty() {
                        "(empty)".into()
                    } else {
                        ascii
                    },
                    hex: if b.is_empty() { "(empty)".into() } else { hex },
                    bytes: b.len(),
                    error: None,
                }
            }
        }
    }

    fn serialize_config(&self) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(256);
        let _ = writeln!(s, "# Serial Terminal config (auto-saved)");
        let _ = writeln!(s, "baud={}", self.baud_text);
        let _ = writeln!(s, "port={}", self.selected_port);
        let _ = writeln!(s, "display_mode={}", display_mode_str(self.display_mode));
        let _ = writeln!(s, "show_ts={}", self.show_ts);
        let _ = writeln!(s, "auto_scroll={}", self.auto_scroll);
        let _ = writeln!(s, "line_ending={}", line_ending_str(self.line_ending));
        let _ = writeln!(s, "dtr={}", self.dtr_on);
        let _ = writeln!(s, "font_size={}", self.console_font_size as i32);
        let _ = writeln!(s, "send_mode={}", send_mode_str(self.send_mode));
        let _ = writeln!(s, "rx_idle_ms={}", self.rx_idle_ms);
        for b in &self.custom_bauds {
            let _ = writeln!(s, "custom_baud={}", b);
        }
        for cmd in &self.history {
            let _ = writeln!(s, "history={}", encode_history(cmd));
        }
        s
    }

    fn autosave_config(&mut self) {
        let now = self.serialize_config();
        if now == self.last_saved_cfg {
            return;
        }
        let Some(path) = config_path() else { return };
        if std::fs::write(&path, &now).is_ok() {
            self.last_saved_cfg = now;
        }
    }
}

// ---------- Serial worker thread ----------
fn serial_worker(
    path: String,
    baud: u32,
    cmd_rx: Receiver<UiCmd>,
    evt_tx: Sender<WorkerEvent>,
    ctx: egui::Context,
) {
    let port_res = serialport::new(&path, baud)
        .timeout(Duration::from_millis(50))
        .open();

    let mut port = match port_res {
        Ok(p) => {
            let _ = evt_tx.send(WorkerEvent::Connected);
            ctx.request_repaint();
            p
        }
        Err(e) => {
            let _ = evt_tx.send(WorkerEvent::Error(format!("Connection failed: {e}")));
            let _ = evt_tx.send(WorkerEvent::Disconnected);
            ctx.request_repaint();
            return;
        }
    };

    let mut buf = [0u8; 4096];
    let mut tx_batch: Vec<u8> = Vec::with_capacity(4096);
    let mut last_flush = Instant::now();

    'outer: loop {
        // 1) Try to read available bytes (50ms timeout configured above)
        match port.read(&mut buf) {
            Ok(0) => {} // unusual on serial
            Ok(n) => {
                tx_batch.extend_from_slice(&buf[..n]);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {
                // normal — no data
            }
            Err(e) => {
                let _ = evt_tx.send(WorkerEvent::Error(format!("Read error: {e}")));
                ctx.request_repaint();
                break 'outer;
            }
        }

        // 2) Flush batch if it has data and is large or aged
        let aged = last_flush.elapsed() >= Duration::from_millis(20);
        if !tx_batch.is_empty() && (tx_batch.len() >= 2048 || aged) {
            let chunk = std::mem::take(&mut tx_batch);
            if evt_tx.send(WorkerEvent::Data(chunk)).is_err() {
                break 'outer;
            }
            ctx.request_repaint();
            last_flush = Instant::now();
        }

        // 3) Drain UI commands
        loop {
            match cmd_rx.try_recv() {
                Ok(UiCmd::Send(bytes)) => match port.write_all(&bytes) {
                    Ok(_) => {
                        let _ = port.flush();
                    }
                    Err(e) => {
                        let _ = evt_tx.send(WorkerEvent::Error(format!("Write error: {e}")));
                        ctx.request_repaint();
                    }
                },
                Ok(UiCmd::SetDtr(level)) => {
                    if let Err(e) = port.write_data_terminal_ready(level) {
                        let _ = evt_tx
                            .send(WorkerEvent::Error(format!("Failed to set DTR: {e}")));
                        ctx.request_repaint();
                    }
                }
                Ok(UiCmd::Disconnect) => break 'outer,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break 'outer,
            }
        }
    }

    // Final flush
    if !tx_batch.is_empty() {
        let _ = evt_tx.send(WorkerEvent::Data(tx_batch));
    }
    let _ = evt_tx.send(WorkerEvent::Disconnected);
    ctx.request_repaint();
    // `port` is dropped here -> handle closed
}

// ---------- Helpers ----------
fn timestamp() -> String {
    Local::now().format("%H:%M:%S%.3f").to_string()
}

fn ascii_from_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

// Make a human-readable preview of raw bytes the user is about to send:
// printable ASCII as-is, common controls as backslash escapes (\r, \n, \t, \\),
// everything else as \xNN. Returns the escaped string and a trailing "(N B)".
fn escape_preview_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + 8);
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7E => out.push(b as char),
            _ => out.push_str(&format!("\\x{:02X}", b)),
        }
    }
    out
}

fn truncate_for_preview(s: &str, max_chars: usize) -> (String, bool) {
    if s.chars().count() <= max_chars {
        (s.to_string(), false)
    } else {
        let mut t: String = s.chars().take(max_chars).collect();
        t.push('…');
        (t, true)
    }
}

// Decode a string with C-style escapes (\n, \r, \t, \0, \\, \" \' and \xNN)
// into raw bytes. Non-escaped characters pass through as their UTF-8 bytes.
// This is the inverse of `escape_preview_bytes` for the printable/escape
// subset, so the converter's two text boxes round-trip cleanly.
fn parse_escaped_ascii(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buf = [0u8; 4];
            let bytes = c.encode_utf8(&mut buf).as_bytes();
            out.extend_from_slice(bytes);
            continue;
        }
        match chars.next() {
            Some('n') => out.push(b'\n'),
            Some('r') => out.push(b'\r'),
            Some('t') => out.push(b'\t'),
            Some('0') => out.push(0),
            Some('\\') => out.push(b'\\'),
            Some('"') => out.push(b'"'),
            Some('\'') => out.push(b'\''),
            Some('x') | Some('X') => {
                let h1 = chars
                    .next()
                    .ok_or_else(|| "expected hex digit after \\x".to_string())?;
                let h2 = chars
                    .next()
                    .ok_or_else(|| "expected second hex digit after \\x".to_string())?;
                let s = format!("{}{}", h1, h2);
                let b = u8::from_str_radix(&s, 16)
                    .map_err(|_| format!("invalid \\x escape: \\x{s}"))?;
                out.push(b);
            }
            Some(other) => return Err(format!("unknown escape: \\{}", other)),
            None => return Err("dangling backslash at end of input".into()),
        }
    }
    Ok(out)
}

// Parse a free-form hex string into bytes. Accepts spaces, tabs, newlines,
// commas, colons, semicolons and `0x` / `0X` prefixes as separators or noise.
// Hex pairs must be two contiguous hex digits each. Examples that all parse to
// the same 5 bytes: "AA 55 01 00 FF", "aa,55,01,00,ff", "0xAA 0x55 0x01 0x00 0xFF",
// "AA:55:01:00:FF", "AA55 0100FF".
fn parse_hex_bytes(input: &str) -> Result<Vec<u8>, String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();

    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        // Skip "0x" / "0X" prefix anywhere in the stream.
        if c == '0' {
            if let Some(&next) = chars.peek() {
                if next == 'x' || next == 'X' {
                    chars.next();
                    continue;
                }
            }
        }
        if c.is_ascii_hexdigit() {
            current.push(c);
        } else if c.is_whitespace() || matches!(c, ',' | ':' | ';' | '-' | '_') {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            return Err(format!("Unexpected character '{}'", c));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    let mut bytes = Vec::with_capacity(tokens.iter().map(|t| (t.len() + 1) / 2).sum());
    for tok in tokens {
        if tok.len() % 2 != 0 {
            return Err(format!(
                "Token '{}' has an odd number of hex digits (need pairs)",
                tok
            ));
        }
        let b = tok.as_bytes();
        for chunk in b.chunks(2) {
            let s = std::str::from_utf8(chunk).unwrap();
            let v = u8::from_str_radix(s, 16)
                .map_err(|_| format!("Invalid hex byte '{}'", s))?;
            bytes.push(v);
        }
    }
    Ok(bytes)
}

fn hex_from_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{:02X}", b));
    }
    s
}

// ---------- Persistent config ----------
// Stored as a small text file next to the executable so the tool stays
// portable (carry the .exe + this file together). Simple key=value format
// so it's human-editable; unknown keys are ignored on load.
const CONFIG_FILE: &str = "cnterminal.cfg";
// Previous installations may have written under these legacy names. Read
// them on first launch so existing users don't lose their settings/custom
// bauds when upgrading; new writes always go to CONFIG_FILE.
const LEGACY_CONFIG_FILE: &str = "serial_terminal.cfg";
const LEGACY_BAUD_FILE: &str = "serial_terminal_bauds.txt";

fn exe_dir() -> Option<std::path::PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

fn config_path() -> Option<std::path::PathBuf> {
    Some(exe_dir()?.join(CONFIG_FILE))
}

#[derive(Default)]
struct AppConfig {
    baud: Option<String>,
    port: Option<String>,
    display_mode: Option<DisplayMode>,
    show_ts: Option<bool>,
    auto_scroll: Option<bool>,
    line_ending: Option<LineEnding>,
    dtr: Option<bool>,
    font_size: Option<f32>,
    custom_bauds: Vec<u32>,
    history: Vec<String>,
    send_mode: Option<SendMode>,
    rx_idle_ms: Option<u32>,
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn parse_display_mode(s: &str) -> Option<DisplayMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "ascii" => Some(DisplayMode::Ascii),
        "hex" => Some(DisplayMode::Hex),
        _ => None,
    }
}

// Escape/unescape for history values stored as a single key=value line.
// Backslash and newlines/CRs are encoded; everything else passes through.
fn encode_history(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(ch),
        }
    }
    out
}

fn decode_history(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn parse_send_mode(s: &str) -> Option<SendMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "text" => Some(SendMode::Text),
        "hex" => Some(SendMode::Hex),
        _ => None,
    }
}

fn send_mode_str(m: SendMode) -> &'static str {
    match m {
        SendMode::Text => "text",
        SendMode::Hex => "hex",
    }
}

fn parse_line_ending(s: &str) -> Option<LineEnding> {
    match s.trim().to_ascii_lowercase().as_str() {
        "none" | "" => Some(LineEnding::None),
        "lf" => Some(LineEnding::Lf),
        "cr" => Some(LineEnding::Cr),
        "crlf" => Some(LineEnding::CrLf),
        _ => None,
    }
}

fn display_mode_str(m: DisplayMode) -> &'static str {
    match m {
        DisplayMode::Ascii => "ascii",
        DisplayMode::Hex => "hex",
    }
}

fn line_ending_str(e: LineEnding) -> &'static str {
    match e {
        LineEnding::None => "none",
        LineEnding::Lf => "lf",
        LineEnding::Cr => "cr",
        LineEnding::CrLf => "crlf",
    }
}

fn load_config() -> AppConfig {
    let mut cfg = AppConfig::default();

    // Primary: the current unified config file. Fall back to the previous
    // name (serial_terminal.cfg) so users carry their settings forward.
    let primary_text = config_path().and_then(|p| std::fs::read_to_string(&p).ok());
    let primary_text = primary_text.or_else(|| {
        exe_dir().and_then(|d| std::fs::read_to_string(d.join(LEGACY_CONFIG_FILE)).ok())
    });

    if let Some(text) = primary_text {
        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let k = k.trim();
            let v = v.trim();
            match k {
                "baud" => cfg.baud = Some(v.to_string()),
                "port" => cfg.port = Some(v.to_string()),
                "display_mode" => cfg.display_mode = parse_display_mode(v),
                "show_ts" => cfg.show_ts = parse_bool(v),
                "auto_scroll" => cfg.auto_scroll = parse_bool(v),
                "line_ending" => cfg.line_ending = parse_line_ending(v),
                "dtr" => cfg.dtr = parse_bool(v),
                "font_size" => cfg.font_size = v.parse::<f32>().ok(),
                "send_mode" => cfg.send_mode = parse_send_mode(v),
                "rx_idle_ms" => cfg.rx_idle_ms = v.parse::<u32>().ok(),
                "custom_baud" => {
                    if let Ok(b) = v.parse::<u32>() {
                        if b > 0 && !BAUDS.contains(&b) && !cfg.custom_bauds.contains(&b) {
                            cfg.custom_bauds.push(b);
                        }
                    }
                }
                "history" => {
                    // Strings can contain anything, but newlines aren't
                    // allowed in the cfg format. Decode \\n -> \n and \\\\ -> \\.
                    let decoded = decode_history(v);
                    if !decoded.is_empty() {
                        cfg.history.push(decoded);
                    }
                }
                _ => {}
            }
        }
    }

    // One-time migration from the old bauds-only file (kept on disk so the
    // user can delete it manually if they wish).
    if let Some(dir) = exe_dir() {
        let legacy = dir.join(LEGACY_BAUD_FILE);
        if let Ok(text) = std::fs::read_to_string(&legacy) {
            for raw in text.lines() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Ok(b) = line.parse::<u32>() {
                    if b > 0 && !BAUDS.contains(&b) && !cfg.custom_bauds.contains(&b) {
                        cfg.custom_bauds.push(b);
                    }
                }
            }
        }
    }

    cfg.custom_bauds.sort_unstable();
    cfg
}

// ---------- Fonts ----------
// egui's built-in fonts cover Latin only, so we register two kinds of system
// fallbacks at startup:
//   1) a Symbol font (Segoe UI Symbol on Windows) for dingbats/arrows like ✕,
//      ↻, ▶, etc. Without this they render as "tofu" □ boxes.
//   2) a CJK font (Malgun Gothic etc.) for Korean glyphs.
// Order matters: symbol font is appended before the CJK one so symbol glyphs
// resolve first when both fonts could theoretically supply them.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    let symbol_candidates: &[(&str, u32)] = &[
        // Windows
        ("C:/Windows/Fonts/seguisym.ttf", 0), // Segoe UI Symbol
        ("C:/Windows/Fonts/segoeui.ttf", 0),  // Segoe UI (also covers many)
        // macOS
        ("/System/Library/Fonts/Apple Symbols.ttf", 0),
        // Linux (often present in DejaVu / Noto stacks)
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 0),
        ("/usr/share/fonts/truetype/noto/NotoSansSymbols-Regular.ttf", 0),
    ];

    let cjk_candidates: &[(&str, u32)] = &[
        // Windows
        ("C:/Windows/Fonts/malgun.ttf", 0),   // Malgun Gothic
        ("C:/Windows/Fonts/malgunbd.ttf", 0), // Malgun Gothic Bold
        ("C:/Windows/Fonts/NanumGothic.ttf", 0),
        ("C:/Windows/Fonts/NotoSansKR-Regular.otf", 0),
        // Linux
        ("/usr/share/fonts/truetype/nanum/NanumGothic.ttf", 0),
        ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc", 1),
        // macOS
        ("/System/Library/Fonts/AppleSDGothicNeo.ttc", 0),
    ];

    register_fallback(&mut fonts, "symbol", symbol_candidates);
    register_fallback(&mut fonts, "cjk", cjk_candidates);

    ctx.set_fonts(fonts);
}

fn register_fallback(
    fonts: &mut FontDefinitions,
    key: &str,
    candidates: &[(&str, u32)],
) {
    for (path, index) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            let mut data = FontData::from_owned(bytes);
            data.index = *index;
            let name = key.to_string();
            fonts.font_data.insert(name.clone(), Arc::new(data));
            if let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
                list.push(name.clone());
            }
            if let Some(list) = fonts.families.get_mut(&FontFamily::Monospace) {
                list.push(name);
            }
            return;
        }
    }
}

// ---------- Theme ----------
fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = PANEL;
    visuals.faint_bg_color = PANEL2;
    visuals.window_stroke = egui::Stroke::new(1.0, LINE);
    visuals.window_corner_radius = 6.0.into();
    visuals.menu_corner_radius = 6.0.into();
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0xff, 0xb0, 0x00, 40);
    visuals.selection.stroke = egui::Stroke::new(1.0, AMBER);
    visuals.hyperlink_color = AMBER;

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = PANEL2;
    w.noninteractive.weak_bg_fill = PANEL2;
    w.noninteractive.bg_stroke = egui::Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = 5.0.into();

    w.inactive.bg_fill = PANEL2;
    w.inactive.weak_bg_fill = PANEL2;
    w.inactive.bg_stroke = egui::Stroke::new(1.0, LINE);
    w.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = 5.0.into();

    w.hovered.bg_fill = PANEL2;
    w.hovered.weak_bg_fill = PANEL2;
    w.hovered.bg_stroke = egui::Stroke::new(1.0, AMBER_DIM);
    w.hovered.fg_stroke = egui::Stroke::new(1.0, AMBER);
    w.hovered.corner_radius = 5.0.into();

    w.active.bg_fill = Color32::from_rgb(0x2a, 0x21, 0x14);
    w.active.weak_bg_fill = Color32::from_rgb(0x2a, 0x21, 0x14);
    w.active.bg_stroke = egui::Stroke::new(1.0, AMBER);
    w.active.fg_stroke = egui::Stroke::new(1.0, AMBER);
    w.active.corner_radius = 5.0.into();

    w.open.bg_fill = PANEL2;
    w.open.weak_bg_fill = PANEL2;
    w.open.bg_stroke = egui::Stroke::new(1.0, AMBER_DIM);
    w.open.fg_stroke = egui::Stroke::new(1.0, TEXT);
    w.open.corner_radius = 5.0.into();

    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(10.0, 6.0);
    style.spacing.interact_size.y = 26.0;

    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(13.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(12.5, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(11.0, FontFamily::Proportional),
    );

    ctx.set_style_of(egui::Theme::Dark, style.clone());
    ctx.set_style_of(egui::Theme::Light, style);
}

// ---------- UI impl ----------
impl eframe::App for SerialApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.pump_events();
        self.maybe_idle_split();
        // Persist user-visible settings whenever anything changes. Cheap:
        // small string, written only on diff.
        self.autosave_config();

        if self.connected {
            // Heartbeat repaint while connected so any missed worker ping
            // still renders within ~100ms.
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        // Wake up exactly when the idle-split window elapses so a single
        // last byte doesn't sit invisible until the next mouse move.
        if self.rx_idle_ms > 0 && !self.rx_partial.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(self.rx_idle_ms as u64));
        }

        // ---------- Top bar ----------
        egui::Panel::top("topbar")
            .frame(
                egui::Frame::default()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(12, 10)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let led_color = if self.connected { GREEN } else { RED };
                    ui.colored_label(led_color, "●");
                    ui.label(
                        RichText::new(APP_NAME)
                            .strong()
                            .color(AMBER)
                            .size(15.0),
                    );

                    ui.add_space(8.0);

                    ui.label(RichText::new("PORT").size(10.0).color(AMBER_DIM));
                    let port_label = if self.selected_port.is_empty() {
                        "—".to_string()
                    } else {
                        self.selected_port.clone()
                    };
                    ui.add_enabled_ui(!self.connected, |ui| {
                        egui::ComboBox::from_id_salt("port_combo")
                            .selected_text(port_label)
                            .show_ui(ui, |ui| {
                                if self.ports.is_empty() {
                                    ui.label(RichText::new("No ports").color(MUTED));
                                }
                                let ports = self.ports.clone();
                                for p in ports {
                                    ui.selectable_value(&mut self.selected_port, p.clone(), p);
                                }
                            });
                    });

                    ui.add_enabled_ui(!self.connected, |ui| {
                        if ui
                            .button(RichText::new("↻").size(13.0))
                            .on_hover_text("Refresh ports")
                            .clicked()
                        {
                            self.refresh_ports();
                        }
                    });

                    ui.add_space(6.0);

                    ui.label(RichText::new("BAUD").size(10.0).color(AMBER_DIM));
                    ui.add_enabled_ui(!self.connected, |ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.baud_text)
                                .desired_width(72.0)
                                .font(TextStyle::Monospace)
                                .hint_text("115200"),
                        );
                        let custom_bauds_snapshot = self.custom_bauds.clone();
                        let mut to_remove: Option<u32> = None;
                        egui::ComboBox::from_id_salt("baud_preset")
                            .selected_text("▼")
                            .width(28.0)
                            .show_ui(ui, |ui| {
                                for &b in BAUDS {
                                    if ui
                                        .selectable_label(
                                            self.baud_text == b.to_string(),
                                            b.to_string(),
                                        )
                                        .clicked()
                                    {
                                        self.baud_text = b.to_string();
                                    }
                                }
                                if !custom_bauds_snapshot.is_empty() {
                                    ui.separator();
                                    ui.label(
                                        RichText::new("CUSTOM").size(10.0).color(AMBER_DIM),
                                    );
                                    for &b in &custom_bauds_snapshot {
                                        ui.horizontal(|ui| {
                                            if ui
                                                .selectable_label(
                                                    self.baud_text == b.to_string(),
                                                    b.to_string(),
                                                )
                                                .clicked()
                                            {
                                                self.baud_text = b.to_string();
                                            }
                                            if ui
                                                .small_button("×")
                                                .on_hover_text("Remove")
                                                .clicked()
                                            {
                                                to_remove = Some(b);
                                            }
                                        });
                                    }
                                }
                            });
                        if let Some(b) = to_remove {
                            self.custom_bauds.retain(|&x| x != b);
                        }
                    });

                    ui.add_space(6.0);

                    let (btn_text, btn_bg, btn_fg, stroke_col) = if self.connected {
                        (
                            "Disconnect",
                            Color32::TRANSPARENT,
                            RED,
                            Color32::from_rgb(0x7a, 0x4a, 0x3a),
                        )
                    } else {
                        (
                            "Connect",
                            AMBER,
                            Color32::from_rgb(0x1a, 0x13, 0x04),
                            AMBER,
                        )
                    };
                    let btn = egui::Button::new(
                        RichText::new(btn_text).color(btn_fg).strong().size(13.0),
                    )
                    .fill(btn_bg)
                    .stroke(egui::Stroke::new(1.0, stroke_col))
                    .min_size(egui::vec2(64.0, 26.0));
                    if ui.add(btn).clicked() {
                        if self.connected {
                            self.disconnect();
                        } else {
                            self.connect(&ctx);
                        }
                    }
                });
            });

        // ---------- Second toolbar row ----------
        egui::Panel::top("toolbar2")
            .frame(
                egui::Frame::default()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let clear_btn = egui::Button::new(
                        RichText::new("Clear").size(11.0).color(TEXT),
                    )
                    .stroke(egui::Stroke::new(1.0, LINE))
                    .min_size(egui::vec2(0.0, 24.0));
                    if ui
                        .add(clear_btn)
                        .on_hover_text("Clear console output")
                        .clicked()
                    {
                        self.clear_console();
                    }
                    ui.add_space(8.0);

                    self.toggle_button(ui, "ASCII", self.display_mode == DisplayMode::Ascii, |s| {
                        s.display_mode = DisplayMode::Ascii;
                    });
                    self.toggle_button(ui, "HEX", self.display_mode == DisplayMode::Hex, |s| {
                        s.display_mode = DisplayMode::Hex;
                    });

                    ui.add_space(6.0);
                    ui.add_enabled_ui(self.connected, |ui| {
                        let dtr_on = self.dtr_on;
                        self.toggle_button(ui, "DTR", dtr_on, move |s| {
                            s.set_dtr(!dtr_on);
                        });
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("FONT").size(10.0).color(AMBER_DIM));
                    let mut size_i = self.console_font_size as i32;
                    egui::ComboBox::from_id_salt("font_combo")
                        .selected_text(format!("{} px", size_i))
                        .width(64.0)
                        .show_ui(ui, |ui| {
                            for &s in FONT_SIZES {
                                ui.selectable_value(&mut size_i, s, format!("{s} px"));
                            }
                        });
                    self.console_font_size = (size_i as f32).clamp(FONT_MIN, FONT_MAX);

                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("RX SPLIT")
                            .size(10.0)
                            .color(AMBER_DIM),
                    )
                    .on_hover_text(
                        "Force a new RX line after this many ms of silence.\nUseful for HEX dumps where bytes have no \\n separators.\n0 = disabled.",
                    );
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.rx_idle_ms_text)
                            .desired_width(44.0)
                            .font(TextStyle::Monospace),
                    );
                    if r.changed() {
                        self.rx_idle_ms = self
                            .rx_idle_ms_text
                            .trim()
                            .parse::<u32>()
                            .unwrap_or(0);
                    }
                    ui.label(RichText::new("ms").size(10.0).color(MUTED));

                    // Right-align: ASCII <-> HEX converter popup toggle.
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let active = self.converter_open;
                            let (fg, stroke_col) = if active {
                                (AMBER, AMBER_DIM)
                            } else {
                                (TEXT, LINE)
                            };
                            let btn = egui::Button::new(
                                RichText::new("ASCII ↔ HEX Converter")
                                    .size(11.0)
                                    .color(fg),
                            )
                            .stroke(egui::Stroke::new(1.0, stroke_col))
                            .min_size(egui::vec2(0.0, 24.0));
                            if ui
                                .add(btn)
                                .on_hover_text("Open ASCII / HEX converter")
                                .clicked()
                            {
                                self.converter_open = !self.converter_open;
                            }
                        },
                    );
                });
            });

        // ---------- Bottom status bar ----------
        egui::Panel::bottom("statusbar")
            .frame(
                egui::Frame::default()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(12, 8)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(MUTED, "STATUS:");
                    let status_color = if self.connected { GREEN } else { MUTED };
                    ui.colored_label(status_color, &self.conn_info);
                    ui.add_space(12.0);
                    ui.colored_label(MUTED, "RX:");
                    ui.colored_label(RX, format!("{} B", self.rx_bytes));
                    ui.add_space(8.0);
                    ui.colored_label(MUTED, "TX:");
                    ui.colored_label(TX, format!("{} B", self.tx_bytes));
                    ui.add_space(8.0);
                    ui.colored_label(MUTED, "LINES:");
                    ui.colored_label(TEXT, format!("{}", self.line_count));

                    // Right-aligned controls. right_to_left layout adds
                    // widgets right-first, so the visual order from left to
                    // right is: [Save Log] [TIME] [AUTO-SCROLL].
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            self.toggle_button(
                                ui,
                                "AUTO-SCROLL",
                                self.auto_scroll,
                                |s| s.auto_scroll = !s.auto_scroll,
                            );
                            self.toggle_button(ui, "TIME", self.show_ts, |s| {
                                s.show_ts = !s.show_ts
                            });
                            ui.add_space(4.0);
                            let save_btn = egui::Button::new(
                                RichText::new("Save Log").size(11.0).color(TEXT),
                            )
                            .stroke(egui::Stroke::new(1.0, LINE))
                            .min_size(egui::vec2(0.0, 22.0));
                            if ui
                                .add(save_btn)
                                .on_hover_text("Save console output to a file")
                                .clicked()
                            {
                                self.save_log();
                            }
                        },
                    );
                });
            });

        // ---------- Bottom send bar ----------
        egui::Panel::right("send_panel")
            .resizable(true)
            .default_size(300.0)
            .min_size(220.0)
            .frame(
                egui::Frame::default()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(10, 10)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("▶ SEND")
                            .size(11.0)
                            .strong()
                            .color(AMBER),
                    );
                    ui.add_space(8.0);
                    let mode = self.send_mode;
                    self.toggle_button(ui, "TEXT", mode == SendMode::Text, |s| {
                        s.send_mode = SendMode::Text;
                    });
                    self.toggle_button(ui, "HEX", mode == SendMode::Hex, |s| {
                        s.send_mode = SendMode::Hex;
                    });

                    ui.add_space(8.0);
                    ui.label(RichText::new("ENDING").size(10.0).color(AMBER_DIM));
                    egui::ComboBox::from_id_salt("ending_combo")
                        .selected_text(self.line_ending.label())
                        .width(72.0)
                        .show_ui(ui, |ui| {
                            for opt in [
                                LineEnding::None,
                                LineEnding::Lf,
                                LineEnding::Cr,
                                LineEnding::CrLf,
                            ] {
                                ui.selectable_value(&mut self.line_ending, opt, opt.label());
                            }
                        });

                    // Right-align: explicit clear button for the input box.
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .add_enabled(
                                    !self.send_text.is_empty(),
                                    egui::Button::new(
                                        RichText::new("× clear").size(10.0).color(MUTED),
                                    )
                                    .stroke(egui::Stroke::new(1.0, LINE))
                                    .min_size(egui::vec2(0.0, 22.0)),
                                )
                                .on_hover_text("Clear input box")
                                .clicked()
                            {
                                self.send_text.clear();
                            }
                        },
                    );
                });
                ui.add_space(4.0);

                // Intercept bare Enter while the editor has focus so it
                // submits instead of inserting a newline. Shift+Enter is
                // left for the TextEdit to handle as a normal newline.
                // The editor is always editable so the user can prepare /
                // tweak commands before opening the port. Enter is only
                // intercepted (and thus consumed) while connected — otherwise
                // it falls through to TextEdit as a regular newline.
                let text_id = ui.make_persistent_id("send_text");
                let focused = ui.memory(|m| m.focused() == Some(text_id));
                let send_via_enter = self.connected
                    && focused
                    && ui.input_mut(|i| {
                        let mut consumed = false;
                        i.events.retain(|e| match e {
                            egui::Event::Key {
                                key: egui::Key::Enter,
                                pressed: true,
                                modifiers,
                                ..
                            } if !modifiers.shift => {
                                consumed = true;
                                false
                            }
                            _ => true,
                        });
                        consumed
                    });

                let editor_rows = 6;
                let hint = match self.send_mode {
                    SendMode::Text => {
                        "Type data — Enter to send, Shift+Enter for newline"
                    }
                    SendMode::Hex => {
                        "Hex bytes, e.g. AA 55 01 00 FF — Enter to send (ENDING is appended)"
                    }
                };
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.send_text)
                        .id(text_id)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .desired_rows(editor_rows)
                        .font(TextStyle::Monospace),
                );

                if send_via_enter {
                    self.send_now();
                    response.request_focus();
                }

                ui.add_space(6.0);

                // WILL SEND preview — shows the exact bytes that go on the
                // wire, both as escaped ASCII and as raw hex so the user can
                // verify either way at a glance.
                {
                    let preview = self.send_preview();
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("WILL SEND")
                                .size(10.0)
                                .color(AMBER_DIM),
                        );
                        if preview.error.is_none() {
                            ui.label(
                                RichText::new(format!("({} B)", preview.bytes))
                                    .size(10.0)
                                    .color(MUTED),
                            );
                        }
                    });
                    let frame = egui::Frame::default()
                        .fill(PANEL2)
                        .stroke(egui::Stroke::new(1.0, LINE))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(6, 4));
                    frame.show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if let Some(err) = preview.error {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("error: {}", err))
                                        .monospace()
                                        .size(11.0)
                                        .color(RED),
                                )
                                .wrap(),
                            );
                        } else {
                            preview_row(ui, "ASCII", &preview.ascii);
                            preview_row(ui, "HEX  ", &preview.hex);
                        }
                    });
                }

                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    let send_btn = egui::Button::new(
                        RichText::new("Send")
                            .strong()
                            .color(Color32::from_rgb(0x1a, 0x13, 0x04)),
                    )
                    .fill(AMBER)
                    .min_size(egui::vec2(80.0, 28.0));
                    if ui.add_enabled(self.connected, send_btn).clicked() {
                        self.send_now();
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("HISTORY")
                            .size(10.0)
                            .color(AMBER_DIM),
                    );
                    if !self.history.is_empty()
                        && ui
                            .small_button("×")
                            .on_hover_text("Clear history")
                            .clicked()
                    {
                        self.history.clear();
                    }
                });

                let mut load_into_editor: Option<String> = None;
                let mut send_immediately: Option<String> = None;
                let mut remove_from_history: Option<usize> = None;

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(f32::INFINITY)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if self.history.is_empty() {
                            ui.label(
                                RichText::new("(no sent commands yet)")
                                    .size(11.0)
                                    .color(MUTED),
                            );
                        }
                        for (idx, cmd) in self.history.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let label = egui::Label::new(
                                    RichText::new(cmd)
                                        .monospace()
                                        .size(12.0)
                                        .color(TEXT),
                                )
                                .truncate()
                                .sense(egui::Sense::click());
                                let r = ui
                                    .add(label)
                                    .on_hover_text(
                                        "Click: load into editor\nDouble-click: send now",
                                    );
                                if r.double_clicked() {
                                    send_immediately = Some(cmd.clone());
                                } else if r.clicked() {
                                    load_into_editor = Some(cmd.clone());
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Remove from history")
                                    .clicked()
                                {
                                    remove_from_history = Some(idx);
                                }
                            });
                        }
                    });

                if let Some(s) = load_into_editor {
                    self.send_text = s;
                }
                if let Some(s) = send_immediately {
                    self.send_text = s;
                    self.send_now();
                }
                if let Some(i) = remove_from_history {
                    if i < self.history.len() {
                        self.history.remove(i);
                    }
                }
            });

        // ---------- ASCII <-> HEX converter popup ----------
        if self.converter_open {
            let mut open = true;
            egui::Window::new("ASCII ↔ HEX Converter")
                .open(&mut open)
                .resizable(true)
                .collapsible(false)
                .default_size([480.0, 320.0])
                .frame(
                    egui::Frame::default()
                        .fill(PANEL)
                        .stroke(egui::Stroke::new(1.0, AMBER_DIM))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(12, 10)),
                )
                .show(&ctx, |ui| {
                    ui.label(
                        RichText::new("Edit either side — the other updates automatically.")
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add_space(6.0);

                    ui.label(
                        RichText::new("ASCII  (escapes: \\n  \\r  \\t  \\0  \\\\  \\xNN)")
                            .size(10.0)
                            .color(AMBER_DIM),
                    );
                    let mut ascii_err: Option<String> = None;
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut self.conv_ascii)
                            .font(TextStyle::Monospace)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY)
                            .hint_text("Type text — use \\xNN for non-printable bytes…"),
                    );
                    if r.changed() {
                        match parse_escaped_ascii(&self.conv_ascii) {
                            Ok(bytes) => {
                                self.conv_hex = hex_from_bytes(&bytes);
                            }
                            Err(e) => ascii_err = Some(e),
                        }
                    }
                    if let Some(e) = ascii_err {
                        ui.label(
                            RichText::new(format!("⚠ {}", e))
                                .size(10.0)
                                .color(RED),
                        );
                    }

                    ui.add_space(8.0);

                    ui.label(
                        RichText::new("HEX bytes")
                            .size(10.0)
                            .color(AMBER_DIM),
                    );
                    let mut hex_err: Option<String> = None;
                    let r = ui.add(
                        egui::TextEdit::multiline(&mut self.conv_hex)
                            .font(TextStyle::Monospace)
                            .desired_rows(4)
                            .desired_width(f32::INFINITY)
                            .hint_text("e.g. 41 42 43 0D 0A"),
                    );
                    if r.changed() {
                        match parse_hex_bytes(&self.conv_hex) {
                            Ok(bytes) => {
                                // Show non-printable bytes as escape sequences
                                // so control bytes (0x00..0x1F, 0x7F+) are
                                // visible and the ASCII side can be edited.
                                self.conv_ascii = escape_preview_bytes(&bytes);
                            }
                            Err(e) => hex_err = Some(e),
                        }
                    }
                    if let Some(e) = hex_err {
                        ui.label(
                            RichText::new(format!("⚠ {}", e))
                                .size(10.0)
                                .color(RED),
                        );
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if ui.button("Copy ASCII").clicked() {
                            ui.ctx().copy_text(self.conv_ascii.clone());
                        }
                        if ui.button("Copy HEX").clicked() {
                            ui.ctx().copy_text(self.conv_hex.clone());
                        }

                        ui.add_space(12.0);

                        if ui
                            .button("→ Send box (TEXT)")
                            .on_hover_text(
                                "Decode ASCII escapes (\\n, \\xNN, …) and load into\nsend editor in TEXT mode. Non-UTF-8 bytes are lossy —\nuse HEX for arbitrary binary.",
                            )
                            .clicked()
                        {
                            // Decode escapes first so users get the actual
                            // text — not a literal "\xNN" string — in the
                            // send box.
                            let bytes = parse_escaped_ascii(&self.conv_ascii)
                                .unwrap_or_else(|_| self.conv_ascii.as_bytes().to_vec());
                            self.send_text = String::from_utf8_lossy(&bytes).into_owned();
                            self.send_mode = SendMode::Text;
                        }
                        if ui
                            .button("→ Send box (HEX)")
                            .on_hover_text("Load HEX into send editor and switch to HEX mode")
                            .clicked()
                        {
                            self.send_text = self.conv_hex.clone();
                            self.send_mode = SendMode::Hex;
                        }

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                if ui.button("Clear both").clicked() {
                                    self.conv_ascii.clear();
                                    self.conv_hex.clear();
                                }
                            },
                        );
                    });
                });
            self.converter_open = open;
        }

        // ---------- Console ----------
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(PANEL)
                    .stroke(egui::Stroke::new(1.0, LINE))
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .outer_margin(egui::Margin::symmetric(12, 6))
                    .corner_radius(8.0),
            )
            .show_inside(ui, |ui| {
                let mut sa = ScrollArea::vertical().auto_shrink([false, false]);
                if self.auto_scroll {
                    sa = sa.stick_to_bottom(true);
                }
                sa.show(ui, |ui| {
                    // Collapse vertical spacing between log lines so more rows
                    // fit on screen. Horizontal spacing inside each line still
                    // matters and is set per-row in render_line.
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.spacing_mut().interact_size.y = 0.0;
                    if self.lines.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(
                                RichText::new("Select a port and click [Connect].")
                                    .color(MUTED)
                                    .size(13.0),
                            );
                            ui.label(
                                RichText::new("Incoming (RX ◄) / outgoing (TX ►) data appears here.")
                                    .color(MUTED)
                                    .size(12.0),
                            );
                        });
                    } else {
                        let font_size = self.console_font_size;
                        for line in &self.lines {
                            render_line(ui, line, self.display_mode, self.show_ts, font_size);
                        }
                    }
                });
            });
    }
}

impl SerialApp {
    fn toggle_button<F: FnOnce(&mut Self)>(
        &mut self,
        ui: &mut egui::Ui,
        label: &str,
        active: bool,
        on_click: F,
    ) {
        let (fg, bg, stroke_col) = if active {
            (
                AMBER,
                Color32::from_rgba_unmultiplied(0xff, 0xb0, 0x00, 16),
                AMBER_DIM,
            )
        } else {
            (MUTED, Color32::TRANSPARENT, LINE)
        };
        let btn = egui::Button::new(RichText::new(label).color(fg).size(11.0))
            .fill(bg)
            .stroke(egui::Stroke::new(1.0, stroke_col))
            .min_size(egui::vec2(0.0, 24.0));
        if ui.add(btn).clicked() {
            on_click(self);
        }
    }
}

fn preview_row(ui: &mut egui::Ui, prefix: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.label(
            RichText::new(prefix)
                .monospace()
                .size(10.0)
                .color(AMBER_DIM),
        );
        ui.add(
            egui::Label::new(
                RichText::new(value)
                    .monospace()
                    .size(11.0)
                    .color(TEXT),
            )
            .wrap(),
        );
    });
}

fn render_line(
    ui: &mut egui::Ui,
    line: &ConsoleLine,
    mode: DisplayMode,
    show_ts: bool,
    font_size: f32,
) {
    let ts_size = (font_size - 2.0).max(8.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
        if show_ts && line.kind != LineKind::Sys {
            ui.label(
                RichText::new(&line.ts)
                    .monospace()
                    .size(ts_size)
                    .color(MUTED),
            );
        }
        let (arrow, color) = match line.kind {
            LineKind::Rx => ("◄", RX),
            LineKind::Tx => ("►", TX),
            LineKind::Sys => ("·", MUTED),
            LineKind::Err => ("×", RED),
        };
        ui.label(
            RichText::new(arrow)
                .monospace()
                .size(font_size)
                .strong()
                .color(color),
        );
        let body = match line.kind {
            LineKind::Rx | LineKind::Tx => match mode {
                DisplayMode::Ascii => ascii_from_bytes(&line.raw),
                DisplayMode::Hex => hex_from_bytes(&line.raw),
            },
            LineKind::Sys | LineKind::Err => line.text.clone(),
        };
        ui.label(
            RichText::new(body)
                .monospace()
                .size(font_size)
                .color(color),
        );
    });
}

// ---------- main ----------
fn main() -> eframe::Result<()> {
    // Procedurally drawn app icon — shown in the title bar and the
    // Windows taskbar. The .exe file's own icon (visible in Explorer)
    // is embedded via build.rs.
    const ICON_SIZE: u32 = 64;
    let icon_rgba = icon::render_rgba(ICON_SIZE);
    let icon_data = egui::IconData {
        rgba: icon_rgba,
        width: ICON_SIZE,
        height: ICON_SIZE,
    };

    let title = format!(
        "{} v{}  ·  by Joseph.han  ·  coding-now.com",
        APP_NAME, APP_VERSION
    );
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_min_inner_size([720.0, 420.0])
            .with_title(title.clone())
            .with_icon(icon_data),
        ..Default::default()
    };
    eframe::run_native(
        &title,
        native_options,
        Box::new(|cc| Ok(Box::new(SerialApp::new(cc)))),
    )
}

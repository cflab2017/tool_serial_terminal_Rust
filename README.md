<h1 align="center">CNTerminal</h1>

<p align="center">
  포터블 시리얼 통신 터미널 · Windows 단일 .exe · 설치 불필요<br/>
  Rust + <a href="https://github.com/emilk/egui">egui</a> · 다크 앰버 CRT 테마
</p>

---

## 최신 버전 다운로드

| 버전 | 배포일 | 설치 파일 | 소스코드 |
|:---:|:---:|:---|:---|
| **v0.1.0** | 2026-05-26 | [📥 CNTerminal_v0.1.0.exe](https://github.com/cflab2017/tool_serial_terminal_Rust/releases/download/v0.1.0/CNTerminal_v0.1.0.exe) | [Source (zip)](https://github.com/cflab2017/tool_serial_terminal_Rust/archive/refs/tags/v0.1.0.zip) · [Source (tar.gz)](https://github.com/cflab2017/tool_serial_terminal_Rust/archive/refs/tags/v0.1.0.tar.gz) |

> **Windows 10 / 11.** 별도 런타임(.NET / WebView2 / Python 등) **불필요**.
> 다운로드한 `.exe` 를 더블클릭하면 바로 실행됩니다.
> 처음 실행 시 SmartScreen 경고가 뜨면 `추가 정보` → `실행` 을 누르세요 (서명 안 된 .exe).
> USB-시리얼 장치는 칩셋 드라이버(CH340 / FTDI / CP210x 등) 가 OS 에 설치돼 있어야 포트가 보입니다.

---

## 주요 기능

### 시리얼 연결
- 포트 자동 검색 + 새로고침
- 보율(baud) **임의 값 입력 가능** + 8 종 프리셋 드롭다운
- 사용자가 입력한 비표준 보율은 자동 저장 → 다음 실행 시 드롭다운에 복원
- DTR 토글 (ESP32/Arduino 자동 리셋 회피, 모뎀 흐름제어 등)

### 콘솔
- RX(초록 ◄) / TX(앰버 ►) / SYS / ERR 색 구분 + 타임스탬프 (HH:MM:SS.mmm)
- **ASCII / HEX 표시 토글** (라인 원본 바이트 보관 → 토글 시 기존 로그도 즉시 재렌더)
- 줄 간격 0 · 폰트 크기 콤보 (9~24 px) → 화면당 라인 수 최대화
- **메모리 보호**: 5,000 줄 상한 자동 트리밍
- 자동 스크롤 (강제 토글 또는 맨 아래일 때만 따라가기)
- **RX SPLIT** — HEX 덤프용. 입력값(ms) 동안 새 바이트가 없으면 자동으로 줄 끊기 (0=비활성)
- Save Log — 콘솔 내용을 .txt 로 저장 (파일 다이얼로그)
- Clear — 콘솔 즉시 비우기

### 송신
- 멀티라인 입력 (포트 연결 전에도 편집 가능)
- **Enter** = 송신 / **Shift+Enter** = 줄바꿈
- ENDING 선택 (None / LF / CR / CRLF) — TEXT·HEX 모드 모두에서 적용
- **TEXT 모드** — 사용자가 입력한 텍스트 그대로 송신 + ENDING append
- **HEX 모드** — `AA 55 01 00 FF`, `aa,55,01`, `0xAA 0x55`, `AA55 0100FF` 등 자유 형식 파싱 → raw 바이트 송신
- **WILL SEND 미리보기** — 실제 와이어로 나가는 바이트를 **ASCII (escape) + HEX** 두 줄로 항상 표시
- 송신 후 입력 박스 보존(같은 명령 반복 송신 편의), `×` 로 수동 클리어
- **송신 히스토리** — 자동 저장(최대 50개), 클릭=로드 / 더블클릭=즉시 송신 / 개별 `×`=제거

### ASCII ↔ HEX 변환기 (팝업)
- 양쪽 박스에 타이핑하면 반대편이 자동 변환
- ASCII 측은 escape 문법 지원: `\n \r \t \0 \\ \"  \xNN`
- 비printable 바이트도 `\xNN` 형식으로 보임 → **round-trip 정확**
- Copy ASCII / Copy HEX 버튼
- `→ Send box (TEXT / HEX)` — 변환 결과를 송신 박스로 로드 + 모드 자동 전환

### 설정 영속화
- 모든 설정이 `.exe` 옆 **`cnterminal.cfg`** (사람이 읽을 수 있는 key=value 텍스트)에 자동 저장
- 다음 실행 시 자동 복원 — 포트/보율/표시 모드/토글/폰트 크기/히스토리/사용자 baud 모두
- 옛 `serial_terminal_bauds.txt` 가 있으면 자동 마이그레이션

---

## 단축키

| 단축키 | 동작 |
|--------|------|
| `Enter` (송신 박스) | 즉시 송신 |
| `Shift+Enter` | 줄바꿈 |

---

## 화면 구성

```
┌──────────────────────────────────────────────────────────────────────────┐
│ ● CNTerminal       PORT [COM3 ▼] ↻   BAUD [115200 ▼]   [ Connect ]       │ ← 1행: 연결
├──────────────────────────────────────────────────────────────────────────┤
│ [Clear] [ASCII][HEX]  [DTR]  FONT[13 px ▼]  RX SPLIT [0] ms              │
│                                                  [ ASCII ↔ HEX Converter] │ ← 2행: 도구
├─────────────────────────────────────┬────────────────────────────────────┤
│                                     │ ▶ SEND  [TEXT][HEX] ENDING [CRLF▼] │
│                                     │ ┌──────────────────────────────┐   │
│                                     │ │ multiline editor             │   │
│   Console                           │ └──────────────────────────────┘   │
│   (RX ◄ / TX ►)                     │ WILL SEND   (5 B)                  │
│                                     │  ASCII  A00\r\n                    │
│                                     │  HEX    41 30 30 0D 0A             │
│                                     │ [ Send ]                           │
│                                     │ ──────────                         │
│                                     │ HISTORY                            │
│                                     │ · AT                               │
│                                     │ · status                           │
├─────────────────────────────────────┴────────────────────────────────────┤
│ STATUS: COM3 @ 115200  RX: 1234 B  TX: 56 B  LINES: 78                   │
│                                       [Save Log] [TIME] [AUTO-SCROLL]    │ ← 푸터
└──────────────────────────────────────────────────────────────────────────┘
```

---

## 빌드 (개발자용)

### 필요 환경
- Rust 1.92 이상 (eframe 0.34 기준)
- Windows 10 / 11 (정식 지원). Linux / macOS 도 빌드 자체는 통과

### 일반 빌드
```bash
cargo build --release
# → target/release/cnterminal.exe
```

### 버전 stamped 빌드 (배포용)
```powershell
.\scripts\build.ps1
# → target/release/cnterminal.exe
# → target/release/CNTerminal_v0.1.0.exe   (Cargo.toml 의 version 자동 추출)
```

### 버전 올리기
1. `Cargo.toml` 의 `[package].version` 만 수정
2. `.\scripts\build.ps1`
3. 윈도우 타이틀바와 빌드 파일명에 버전이 자동 반영됨

---

## 기술 요약

| 항목 | 선택 / 이유 |
|------|-------------|
| GUI | **eframe + egui** — 모든 것을 정적 링크해 단일 .exe. WebView2/.NET/런타임 전부 불필요 |
| 시리얼 | **serialport** 크레이트 (cross-platform) |
| 스레드 | UI ↔ 워커 스레드를 `mpsc` 채널 2개로 분리. 워커는 50ms 타임아웃 read + 20ms 배치 |
| 폰트 | 시스템 한글(맑은 고딕)·Symbol(Segoe UI Symbol) 폰트를 런타임 fallback 으로 등록 |
| 아이콘 | 절차적 RGBA 생성 → 창 아이콘 + 다중 사이즈 ICO 를 PE 리소스로 임베드 |
| 설정 | exe 옆 `cnterminal.cfg` (key=value 텍스트) 매 프레임 diff 후 변경 시에만 저장 |

---

## 라이선스 / 제작자

- **Joseph.han** · [coding-now.com](https://coding-now.com)
- Issues / Pull requests welcome.

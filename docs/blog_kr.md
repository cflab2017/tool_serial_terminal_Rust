---
title: "CNTerminal — 설치 없이 바로 쓰는 포터블 시리얼 터미널"
date: 2026-05-26
tags: [serial, terminal, embedded, tool, rust, egui]
lang: ko
---

# CNTerminal — 설치 없이 바로 쓰는 포터블 시리얼 터미널

![CNTerminal](./CNTerminal.png)

> **한 줄 요약**: 다운로드 → 더블클릭 → 끝. 약 8 MB 짜리 단일 `.exe` 한 개로
> 시리얼 모니터링, HEX 송수신, ASCII↔HEX 변환까지 다 됩니다.
> 설치 프로그램·런타임·관리자 권한 전부 필요 없습니다.

USB-시리얼 디바이스(아두이노, ESP32, 모뎀, 산업용 센서 등)와 통신할 때 매번 PuTTY
설정을 손보거나, Arduino IDE 시리얼 모니터의 부족한 기능에 답답해하신 적 있다면 —
**CNTerminal** 을 한 번 써 보세요. USB 메모리에 넣어 다니거나 사내 공유폴더에 두고
누구나 더블클릭으로 바로 쓸 수 있게 설계됐습니다.

- 💾 **다운로드**: [CNTerminal_v0.1.0.exe](https://github.com/cflab2017/tool_serial_terminal_Rust/releases/latest)
- 📂 **소스코드**: [GitHub](https://github.com/cflab2017/tool_serial_terminal_Rust)
- 🎮 **인터랙티브 데모**: [demo.html](./demo.html) (브라우저에서 바로 동작)

---

## 1. 설치 (라기엔 거창한)

1. [Releases 페이지](https://github.com/cflab2017/tool_serial_terminal_Rust/releases) 에서
   `CNTerminal_v0.1.0.exe` 다운로드
2. 원하는 폴더에 두고 더블클릭
3. 끝

처음 실행 시 윈도우 SmartScreen 경고가 뜨면 **추가 정보 → 실행** 을 누르세요
(코드 서명이 안 된 .exe 라서 그렇습니다 — 소스가 공개되어 있으니 직접 빌드해 쓰셔도 됩니다).

USB-시리얼 칩셋 드라이버(CH340 / CP210x / FTDI 등)는 Windows 가 보통 자동으로
잡아주지만, 포트 목록에 안 보이면 칩 제조사 드라이버를 먼저 설치해야 합니다.

---

## 2. 5분 워크스루

### 2-1. 포트 연결

1. 상단의 **PORT** 드롭다운에서 디바이스 포트 선택 (`↻` 로 새로고침)
2. **BAUD** 입력 — 프리셋(`9600` ~ `921600`)은 우측 `▼` 클릭, 비표준 값은 직접 타이핑 (예: `500000`)
3. **Connect** 버튼

연결되면 좌측 LED 가 빨강 → **초록** 으로 바뀌고, RX 데이터가 곧바로 콘솔에
실시간 출력됩니다.

### 2-2. 데이터 보내기

오른쪽 **▶ SEND** 패널의 멀티라인 박스에 명령을 입력하고 **Enter** 를 누르세요.

- `Enter` → 즉시 송신
- `Shift+Enter` → 줄바꿈 (멀티라인 입력)
- `ENDING` 콤보로 줄끝 선택 (`None` / `LF` / `CR` / `CRLF`)
- 보내자마자 입력 박스가 **비워지지 않습니다** — 같은 명령을 또 보내려면 Enter 만 한 번 더 누르면 됩니다
- 손으로 지우려면 헤더 우측의 `× clear` 버튼

### 2-3. 진짜로 어떻게 나가는지 한눈에

입력 박스 바로 아래 **WILL SEND** 미리보기가 실시간으로:

```
WILL SEND   (5 B)
ASCII  A00\r\n
HEX    41 30 30 0D 0A
```

ASCII (escape 인코딩) 과 HEX 둘 다 항상 함께 보여줍니다. ENDING 까지 포함된
**실제 와이어로 나가는 바이트** 가 그대로 보이기 때문에 송신 실수를 송신 *전* 에
잡아낼 수 있습니다.

---

## 3. 자주 쓰는 시나리오

### 시나리오 ①: 아두이노 시리얼 모니터링

```
BAUD     115200
ENDING   LF
MODE     TEXT
```

스케치에서 `Serial.println("temp=25.3")` 같이 보내면 그대로 한 줄씩 표시.
타임스탬프가 자동으로 붙고, **AUTO-SCROLL** 토글로 자동 추적,
**Save Log** 로 .txt 백업.

### 시나리오 ②: HEX 프로토콜 디버깅 (STX/ETX, 체크섬)

산업용 센서나 PLC 가 `0x02 ... 0x03` 같은 STX/ETX 로 감싼 바이너리
프레임을 쓸 때 — 일반 ASCII 모니터는 깨진 글자만 보여서 디버깅이 힘듭니다.

**CNTerminal 사용법**:

1. 상단 둘째 줄에서 **HEX** 토글 → 콘솔이 hex 덤프 표시로 전환
2. **RX SPLIT** 에 `2` 입력 → 2 ms 동안 새 바이트가 안 오면 자동으로 줄을 끊습니다
   (HEX 모드에선 `\n` 같은 자연 구분자가 없어 한 줄이 끝없이 늘어지는 문제 해결)
3. 송신은 우측 패널의 **HEX** 모드 → `02 41 35 30 30 03` 처럼 입력하면 raw 바이트로 송신
   (ENDING 까지 자동으로 뒤에 붙음)

### 시나리오 ③: ESP32 자동 리셋 회피

ESP32/Arduino 는 DTR 라인의 변화로 자동 리셋되도록 회로가 구성된 경우가
많습니다. 디버그 중에 의도치 않은 리셋을 막으려면:

1. 연결 후 둘째 줄의 **DTR** 토글을 한 번 눌러 **OFF** 상태로 (회색)
2. 이제 연결/해제와 무관하게 DTR 라인이 가만히 있어 보드가 리셋되지 않음

---

## 4. ASCII ↔ HEX 변환기

둘째 툴바 우측의 **`ASCII ↔ HEX Converter`** 버튼을 누르면 별도 팝업이
열립니다. 양쪽 박스 중 어디든 타이핑하면 반대편이 자동 변환:

| 입력 | 결과 |
|------|------|
| ASCII: `\x02A50000\x03` | HEX: `02 41 35 30 30 30 30 03` (8 B) |
| ASCII: `Hello\nWorld` | HEX: `48 65 6C 6C 6F 0A 57 6F 72 6C 64` |
| HEX: `02 41 35 30 30 03` | ASCII: `\x02A500\x03` |

**핵심**: 비printable 바이트도 `\xNN` 형식으로 보이기 때문에 round-trip 이
정확합니다. STX/ETX 가 들어간 프레임을 분석할 때 유용.

`→ Send box (TEXT/HEX)` 버튼으로 변환 결과를 송신 박스에 바로 로드 +
모드 자동 전환까지.

---

## 5. 설정은 자동 저장 (포터블 그대로)

조정한 모든 설정 — 포트, 보율, 폰트 크기, 송신 히스토리, 사용자 추가 baud 까지 —
이 **`.exe` 옆 `cnterminal.cfg`** 한 파일에 자동 저장됩니다.

```text
# Serial Terminal config (auto-saved)
baud=500000
port=COM3
display_mode=ascii
font_size=13
custom_baud=500000
history=AT
history=status
```

`.exe` + `cnterminal.cfg` 두 파일을 USB 에 넣어 다니면 어디서든 같은 환경.

---

## 6. 단축키 / 토글 정리

| 위치 | 동작 |
|------|------|
| 송신 박스에서 `Enter` | 즉시 송신 |
| 송신 박스에서 `Shift+Enter` | 줄바꿈 |
| 둘째 툴바 `Clear` | 콘솔 비우기 |
| 둘째 툴바 `ASCII / HEX` | 콘솔 표시 모드 전환 (raw 보존, 재렌더) |
| 둘째 툴바 `DTR` | DTR 라인 ON/OFF |
| 둘째 툴바 `FONT [13 px ▼]` | 콘솔 폰트 크기 |
| 둘째 툴바 `RX SPLIT [n] ms` | n ms 침묵 시 자동 줄 끊기 (0 = 비활성) |
| 푸터 `Save Log` | 콘솔 내용 .txt 저장 |
| 푸터 `TIME` | 타임스탬프 표시 토글 |
| 푸터 `AUTO-SCROLL` | 자동 스크롤 토글 |

---

## 7. 라이브 데모

블로그에서 바로 동작하는 인터랙티브 데모를 준비했습니다 (가짜 디바이스로
시뮬레이션, 실제 USB 시리얼은 필요 없음):

```html
<iframe src="./demo.html"
        width="100%" height="640"
        style="border:1px solid #2a251e;border-radius:6px"
        title="CNTerminal demo"></iframe>
```

→ **[데모 바로 보기](./demo.html)**

데모에서 해 볼 수 있는 것:

- `Connect` 누르면 가짜 센서 데이터 스트림이 자동 시작
- 송신 박스에 `AT`, `status`, `help`, 또는 임의 문자열 입력 → Enter
- `ASCII` / `HEX` 표시 토글
- `TEXT` / `HEX` 송신 모드, `ENDING` 변경
- WILL SEND 미리보기 실시간 갱신

---

## 8. 빌드해서 직접 쓰고 싶다면

소스가 완전히 공개돼 있고, 외부 이미지 자산 없이 아이콘도 코드로 그려서
임베드되니 직접 빌드도 깔끔합니다.

```powershell
git clone https://github.com/cflab2017/tool_serial_terminal_Rust
cd tool_serial_terminal_Rust
cargo build --release         # → target/release/cnterminal.exe
# 또는 버전 stamped 빌드 (배포용)
.\scripts\build.ps1           # → target/release/CNTerminal_v0.1.0.exe
```

**필요 환경**: Rust 1.92+, Windows 권장 (Linux/macOS 도 빌드 자체는 통과).
약 30 초 ~ 2 분 (의존성 캐시 상태에 따라).

---

## 9. 기술 메모 (관심 있는 분만)

- **GUI**: [eframe + egui](https://github.com/emilk/egui). 모든 것을 정적 링크해서 단일 .exe
  완성 — WebView2 / .NET / Python 같은 외부 런타임 0 개
- **시리얼**: `serialport` 크레이트 (cross-platform)
- **아키텍처**: UI 스레드와 시리얼 워커 스레드를 `std::sync::mpsc` 채널 2개로
  분리. 워커는 50 ms 타임아웃 read + 20 ms 배치로 묶어 UI 부담 최소화.
  데이터 도착 시 `ctx.request_repaint()` 로 정확한 시점에 다시 그림.
- **메모리 보호**: 콘솔 5,000 줄 상한 자동 트리밍
- **테마**: 다크 앰버 CRT 톤 (#0b0a08 배경 / #ffb000 강조), CSS scanline 같은
  레트로 분위기는 의도적으로 자제

자세한 코드: [src/main.rs](https://github.com/cflab2017/tool_serial_terminal_Rust/blob/main/src/main.rs)

---

## 마무리

만들 때 가장 신경 쓴 건 **"열자마자 다 보이는 도구"** 였습니다.
설정 메뉴 깊숙이 숨겨진 토글, 사용 설명서를 읽어야만 알 수 있는 단축키,
환경 변수, 별도 설치 — 이런 것들을 다 빼고 핵심만 한 화면에 펴 놓은
시리얼 터미널이 필요했습니다.

피드백/이슈/PR 환영합니다. ([GitHub Issues](https://github.com/cflab2017/tool_serial_terminal_Rust/issues))

— **Joseph.han** · [coding-now.com](https://coding-now.com)

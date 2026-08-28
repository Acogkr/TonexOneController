# TONEX ONE Controller 공연 화면 재설계 보고서

작성일: 2026-07-31  
대상 검증 보드: JC3248W535, 480×320  
적용 범위: Rust 공용 UI 렌더러가 지원하는 모든 디스플레이 클래스

## 1. 목표

공연 중 서 있는 사용자가 한눈에 다음 네 가지를 파악할 수 있는 화면을 만든다.

1. 현재 프리셋 번호와 이름
2. 현재 TONEX ONE 슬롯 A/B/C
3. AMP, CAB, FX의 활성 상태와 신호 순서
4. Master Volume, BPM, 설정 진입점

공연 화면에는 상태 확인에 필요한 정보만 남긴다. 파라미터 편집, Wi-Fi 설정,
QR 연결, 튜너는 별도 화면으로 분리한다.

## 2. 공식 제품 조사

### Neural DSP Quad Cortex

- `The Grid`는 AMP, CAB, FX를 동일한 블록 체계로 표시한다.
- 현재 프리셋은 상단, 현재 Scene은 상단 우측에 둔다.
- 공연용 `Gig View`는 화면 전체를 풋스위치 할당의 큰 요약으로 사용한다.
- 공연 화면과 블록 편집 화면을 분리한다.

출처:
[Quad Cortex User Manual 4.0.0](https://neuraldsp.com/manual/quad-cortex),
[Gig View 소개](https://neuraldsp.com/quad-cortex-updates/gig-view-quad-cortex-development-update)

### Line 6 Helix Stadium / Helix / HX Stomp

- 최신 Helix Stadium은 `Home > Play`와 `Home > Edit`를 분리한다.
- Play 화면은 공연 중 필요한 풋스위치 라벨에 집중한다.
- Edit 화면은 신호 흐름 순서의 동일 크기 블록을 사용한다.
- 프리셋 번호·이름과 Snapshot은 상단에 유지한다.
- 선택 상태는 두꺼운 테두리, 활성 상태는 별도의 색으로 구분한다.

출처:
[Helix Stadium Display](https://manuals.line6.com/en/helix-stadium/live/display),
[Helix 3.80 Owner's Manual](https://line6.com/data/6/0a00051afda2673ccc1cc8e68/application/pdf/Helix%203.80%20Owner%27s%20Manual%20-%20English.pdf),
[HX Stomp Owner's Manual](https://line6.com/data/6/0a020a4010c935bb66a4c0c44f/application/pdf/HX)

### BOSS GX-10 / GX-100

- Memory Number, Memory Name, Control, Chain, Tuner 화면을 분리한다.
- Chain 화면은 신호 배치 확인에 집중한다.
- Control 화면은 표시 요소와 실제 조작 대상을 직접 대응시킨다.
- 제한된 색상 모드에서도 상태를 구분할 수 있도록 색 이외의 대비를 제공한다.

출처:
[GX-10 Play Screen](https://static.roland.com/manuals/gx-10_reference/en-US/7330471594710027.html),
[GX-10 Memory Selection](https://static.roland.com/manuals/gx-10_reference/en-US/7270887594706187.html),
[GX-10 Color Modes](https://static.roland.com/manuals/gx-10_reference/en-US/8541914794720267.html),
[GX-100 제품 및 지원](https://www.boss.info/global/products/gx-100/support/)

### HeadRush Prime

- 메인 Rig 화면은 블록 기반 신호 체인이 중심이다.
- 블록을 직접 선택한 뒤 별도 편집 상태로 이동한다.
- Rig, Stomp, Hybrid, Setlist, Song 역할을 분리한다.
- 실제 풋스위치의 색과 화면 라벨을 대응시킨다.

출처:
[HeadRush Prime 공식 제품 페이지](https://www.headrushfx.com/products/prime/index.html)

### Fractal FM3

- 일반 Home 화면과 공연용 Performance Page를 구분한다.
- 서 있는 위치에서 읽기 위한 Large Fonts 표시를 제공한다.
- Performance Page는 공연 중 필요한 소수의 값을 사용자 선택으로 유지한다.
- 세부 편집 기능을 공연 화면에 모두 노출하지 않는다.

출처:
[FM3 Owner's Manual](https://www.fractalaudio.com/downloads/manuals/FM3/FM3-Owners-Manual.pdf)

### Kemper Profiler Stage

- Performance, Slot, Rig를 계층적으로 표시한다.
- 제한된 화면에서는 Rig 이름 중심 또는 Signal Chain 중심 레이아웃을 선택한다.
- 전체 편집은 Rig Manager로 분리할 수 있다.

출처:
[Kemper 공식 FAQ](https://www.kemper-amps.com/faqs),
[Profiler Stage Quick Start](https://www.kemper-amps.com/download/static/263/Profiler%20Stage%20Quick%20Start%202021.pdf)

## 3. 조사에서 도출한 공통 원칙

1. 공연 화면과 편집 화면을 분리한다.
2. 프리셋 이름과 현재 Scene/Slot이 최우선 정보다.
3. AMP/CAB/FX는 신호 흐름의 동등한 구성 요소로 표현한다.
4. 장식용 외곽 카드보다 동일한 블록과 일정한 간격을 사용한다.
5. 켜짐, 꺼짐, 선택됨은 서로 다른 시각 규칙을 사용한다.
6. 화면에 보이는 버튼은 실제로 눌려야 한다. 상태만 보여주는 요소는 버튼처럼
   입체적으로 그리지 않는다.
7. 공연용 글자는 서 있는 거리에서 읽히는 크기를 우선한다.
8. 부가 정보는 별도 화면으로 보내고 한 화면의 텍스트 수를 제한한다.

## 4. 현재 화면 정량 감사

현재 480×320 렌더링의 실제 배치는 다음과 같다.

| 영역 | 좌표 및 크기 | 문제 |
|---|---:|---|
| 프리셋 번호 | x24–82, y24–72, 58×48 | 본문 패널과 하단 경계가 맞닿음 |
| 슬롯 A/B/C | x398–456, y24–72, 58×48 | 본문 패널과 하단 경계가 맞닿음 |
| 프리셋 이름 | x89–391, 높이 40px | 양쪽 배지와 광학적 무게가 다름 |
| 중앙 외곽 패널 | x24–456, y72–230, 432×158 | 상단 간격 0px, 정보 없는 래퍼 |
| AMP/CAB | y80–147, 높이 67px | FX와 같은 높이지만 래퍼 안에 중첩 |
| FX | y155–222, 높이 67px | 버튼처럼 보이지만 보드 탭 동작 없음 |
| 빈 구간 | y230–272, 높이 42px | 전체 화면의 13.1% |
| Footer | y272–304, 높이 32px | 상태와 설정이 하단에 고립 |

외곽 패널의 정보 없는 면적과 하단 공백을 합치면 화면의 약 23.7%가
비기능 공간이다. 반대로 헤더와 중앙 패널의 간격은 0px이다.

SETTINGS의 표시 영역은 82×32이지만 실제 터치 판정은 약 132×54로 서로
다르다. 보이지 않는 왼쪽·위쪽 공간에서도 설정이 열릴 수 있다.

## 5. 폐기하는 설계

- AMP/CAB를 높이로 강조하는 2행 카드
- 중앙 전체를 감싸는 장식용 외곽 패널
- 프리셋 번호와 슬롯을 큰 채움 박스로 동시에 표시
- 상태 요소를 실제 버튼처럼 보이게 하는 입체 카드
- 화면을 채우기 위해 Master/BPM/설명을 반복 표기
- 표시 영역과 실제 터치 영역이 다른 SETTINGS

## 6. 채택 설계: Signal Chain Rail

### 6.1 정보 구조

화면을 세 그룹으로만 나눈다.

1. Open Header: 프리셋 번호, 이름, 현재 슬롯
2. Signal Chain Rail: GATE → COMP → AMP → CAB → MOD → DLY → REV
3. Performance Footer: Master Volume, BPM, 설정

AMP와 CAB도 FX와 완전히 동일한 크기로 표시한다. 신호 흐름 순서에 맞추기
위해 GATE와 COMP 뒤, MOD/Delay/Reverb 앞에 배치한다.

### 6.2 480×320 기준 와이어프레임

```text
┌──────────────────────────────────────────────────────────────┐
│ ⚙                 JCM 800 LEAD                         08-A │
│                                                              │
│ GATE ─ COMP ─ AMP ─ CAB ─ MOD ─ DLY ─ REV                   │
│                                                              │
│ VOL  -8.5 dB    BPM  118                                      │
└──────────────────────────────────────────────────────────────┘
```

### 6.3 480×320 픽셀 계획

| 요소 | 좌표 | 크기/규칙 |
|---|---:|---|
| Safe area | x16–464, y14–306 | 외곽 14–16px |
| Settings gear | x10–62, y16–68 | 52×52 터치 영역 안의 27px 톱니바퀴 |
| Preset name | x102–370, y14–70 | 광학 중심 정렬, 26px |
| Preset/Slot | x378–464, y14–70 | 같은 26px 폰트의 `01-A` 형식 |
| Header→Rail | y70–86 | 16px |
| Rail | x16–464, y86–180 | 448×94 |
| Rail tile | 7개 | 정확히 58×94, 간격 정확히 7px |
| Rail→Footer | y180–238 | 의도적인 58px 휴식 공간 |
| Master | x16–128, y238–270 | `VOL -8.5 dB` 소형 metadata |
| BPM | x144–216, y238–270 | `BPM 118` 소형 metadata |
| Back | x408–464, y223–279 | Settings 화면 전용 56×56 |

### 6.4 간격 규칙

- 화면 외곽: 16px
- 동일 그룹 내부: 7px
- 독립 그룹 사이: 최소 14px
- Header와 Rail: 16px
- Rail과 Footer: 18px
- 어떤 정보 블록도 다른 그룹 경계와 0px로 맞닿지 않는다.

### 6.5 타이포그래피

| 정보 | 우선순위 | 480×320 목표 |
|---|---:|---:|
| 프리셋 이름 | 1 | FONT_7X13_BOLD 2배, 26px |
| 프리셋 번호·슬롯 | 2 | FONT_7X13_BOLD 2배, 26px |
| 신호 블록 | 3 | FONT_10X20, 20px |
| Master/BPM 값 | 3 | 20px |
| 프리셋 번호·보조 label | 4 | 10–18px |

현재 내장 폰트의 실제 정수 배율만 사용한다. 구현할 수 없는 28–32px 같은
중간 크기를 가정하지 않는다. 프리셋 이름이 길면 저장 단계와 표시 단계 모두
UTF-8 경계를 지키면서 축소 또는 말줄임한다. 다른 영역을 침범하지 않는다.

### 6.6 상태 색

- ON: 프로필 accent 채움 + 어두운 글자
- OFF: 배경과 구분되는 저채도 면 + 밝은 글자
- 선택됨: 밝은 2–3px outline
- 연결 끊김: 전체를 지우지 않고 상태 색만 muted
- 색만으로 상태를 표현하지 않고 채움/outline/명도도 함께 바꾼다.

### 6.7 상호작용 규칙

- Stage 화면의 Chain Rail은 풋스위치 할당이나 터치 버튼이 아닌 상태 표시다.
  그림자, 눌림 애니메이션, 입체 버튼 표현을 사용하지 않는다. 얇은 연결선 위의
  평면 상태 블록으로 그린다.
- SETTINGS 표시 영역과 실제 hit rectangle은 공용 geometry의 동일 `Rect`를
  사용하며 `contains(x, y)`로 네 경계를 모두 검사한다.
- 480×320 SETTINGS gear의 hit rect는 52×52이다.
- Settings 화면의 BACK은 QR과 겹치지 않는 별도 56×56 `Rect`를 사용한다.
- TouchInterpreter는 현재 화면을 몰라도 SETTINGS와 BACK 두 `Rect`를 모두 동일한
  page-toggle action으로 판정한다.
- UI model은 실제 보드의 touch capability를 받아야 한다. 비터치 보드에서는
  SETTINGS/BACK을 터치 버튼처럼 표시하지 않는다.
- 향후 Chain Rail 탭 편집을 추가할 때만 전체 타일을 터치 영역으로 승격한다.

## 7. 반응형 적용

모든 보드에 같은 정보 구조를 유지하되, 단순 축소만 하지 않는다.

| 클래스 | Rail 배치 | 라벨 |
|---|---|---|
| 800×480 | 7개 단일 행 | 전체 라벨 |
| 480×320 | 7개 단일 행, 58px tile + 7px gap | 전체 또는 3–4자 |
| 320×170 | 7개 단일 행 | 1–4자 축약 |
| 280×240 | 4+3 두 행 허용 | 1–4자 축약 |
| 240×280 | 4+3 두 행 허용 | 1–3자 축약 |
| 128×128 | 4+3 두 행 | 1자 상태 코드 |

화면이 좁아져도 정보 우선순위는 `Preset → Slot → Chain → Master/BPM → Settings`
순서를 유지한다.

## 8. 구현 순서

1. 기존 외곽 중앙 패널과 큰 배지 채움을 제거한다.
2. Stage 전용 layout geometry를 순수 함수로 분리한다.
3. 480×320 Signal Chain Rail을 구현한다.
4. 좁은 화면은 4+3 responsive rail로 구현한다.
5. `UiViewModel`에 실제 `has_touch` capability를 전달한다.
6. SETTINGS와 BACK의 visual rectangle을 UI model에서 단일 소스로 만든다.
7. TouchInterpreter가 동일 geometry와 네 경계 `contains`를 사용하도록 변경한다.
8. 각 rectangle의 겹침, 간격, 최소 크기를 테스트한다.
9. 전 화면 클래스 BMP를 생성해 시각 검토한다.
10. 독립 AI 검토의 지적을 반영한다.
11. JC3248W535에 한 번 플래시하고 실물 거리·터치를 확인한다.

## 9. 완료 판정 기준

- [x] Header와 Rail 사이가 모든 보드에서 0px보다 크다.
- [x] 480×320에서 Header→Rail 간격이 14px 이상이다.
- [x] 480×320의 7개 Rail tile 크기가 동일하다.
- [x] AMP/CAB가 어떤 FX보다 크지 않다.
- [x] 중앙 장식용 외곽 패널이 없다.
- [x] 480×320에서 7개 Rail tile은 정확히 58×94이고 간격은 정확히 7px이다.
- [x] 프리셋 이름이 number/slot 영역을 침범하지 않는다.
- [x] SETTINGS visual rect와 hit rect가 동일하다.
- [x] SETTINGS/BACK hit rect의 네 경계와 각 ±1px 테스트가 통과한다.
- [x] 비터치 보드에는 터치 버튼 affordance가 없다.
- [x] 보이는 모든 버튼은 실제 동작하고, 상태 전용 요소는 버튼처럼 보이지 않는다.
- [x] disconnected 상태에서 OFFLINE 표식, muted rail, stale 값 `--`가 표시된다.
- [x] 긴 ASCII, 한글, 32바이트 UTF-8 경계 이름이 다른 영역을 침범하지 않는다.
- [x] 모든 UI 클래스의 렌더 경계·겹침 테스트가 통과한다.
- [ ] JC3248W535 실물에서 프리셋, 슬롯, Chain 상태를 서서 읽을 수 있다.
- [ ] Settings 진입과 BACK이 각 한 번의 탭으로 동작한다.
- [ ] 실물에서 Settings/BACK 각각 20회 탭 성공률이 100%다.

자동 검증 근거:

- `tonex-ui-model`: 모든 화면 클래스의 단일 geometry와 UTF-8 경계 테스트
- `tonex-ui-renderer`: 전체 화면 클래스 렌더 경계·블록 겹침·58×94/7px 테스트
- `tonex-controls`: SETTINGS 네 경계 안쪽 및 바깥쪽 ±1px 터치 테스트
- `cargo test --workspace`: 전체 통과
- JC3248W535(COM4, ESP32-S3)에 `board-jc3248w535,wifi-web` 릴리스 펌웨어 플래시 완료

남은 세 항목은 실제 패널의 시야거리와 정전식 터치 센서 결과가 필요한 실물 검증이다.

## 10. 설계 결정

추천안은 `Signal Chain Rail`이다. 최신 멀티 이펙터의 Play/Gig/Chain 화면에서
공통으로 확인되는 신호 흐름, 동일 블록, 공연/편집 분리 원칙을 가장 직접적으로
반영한다. 현재 화면의 반복된 문제인 AMP/CAB 과대, 박스 접촉, 정보 없는 래퍼,
하단 공백, 허위 버튼 표현을 동시에 제거한다.

## 11. 독립 AI 검토 결과와 반영

독립 검토는 Signal Chain Rail 방향에는 동의했지만, 초안의 수치와 capability
모델에 세 가지 P0 문제를 발견했다.

1. 448px Rail의 잔여 6px가 정의되지 않음
2. SETTINGS/BACK 표시와 TouchInterpreter 판정이 서로 다른 좌표를 사용함
3. 같은 화면 클래스라도 touch가 없는 보드에 가짜 터치 버튼이 표시될 수 있음

다음과 같이 수정했다.

- Rail은 `7×58 + 6×7 = 448`로 정확히 닫는다.
- SETTINGS와 BACK은 각각의 공용 `Rect`를 렌더러와 TouchInterpreter가 공유한다.
- `UiViewModel`은 실제 `has_touch`를 받는다.
- 과도했던 그룹 공백 22px/38px를 16px/18px로 줄였다.
- 폰트 목표를 실제 내장 폰트와 정수 배율에 맞췄다.
- Slot은 `SLOT A`가 아니라 큰 `A` 하나만 표시한다.
- SETTINGS는 64×64에서 56×56으로 줄였다.
- UTF-8, offline, touch/no-touch, hit 경계 ±1px 검증을 완료 기준에 추가했다.

## 12. 실물 피드백 후 시각 정제

첫 Signal Chain Rail은 정보 구조와 간격 문제는 해결했지만, 활성 블록 전체를
프로필 색으로 채워 큰 색 덩어리 일곱 개가 연속되었다. 이 방식은 공연 중 ON/OFF
구분은 강하지만 산업용 상태판처럼 무겁고 투박해 보인다는 실물 피드백을 받았다.

정보 구조와 좌표는 유지하고 시각 표현만 다음과 같이 정제했다.

- 활성 블록 전체 색 채움을 제거하고 어두운 공통 표면을 사용한다.
- 활성 상태는 2px 프로필색 outline, 3px 상단 accent line, 작은 상태점으로 표시한다.
- 비활성 블록은 1px 저채도 outline과 muted label만 남긴다.
- AMP, CAB, 모든 FX는 동일 크기·모서리·상태 규칙을 공유한다.
- 블록 라벨은 20px에서 18px로 한 단계 가볍게 조정한다.
- SETTINGS/BACK은 낮은 명도의 1px outline과 11px corner radius만 사용한다.
- Master/BPM은 큰 값과 작은 label의 2단 구성을 버리고 20px 한 줄 상태 정보로 낮춘다.
- SETTINGS는 크기를 줄여 터치 신뢰성을 잃지 않도록 56×56을 유지하되 Master/BPM과
  같은 수평 중심선으로 옮기고, 별도의 accent bar를 제거해 존재감을 낮춘다.
- 프리셋, 슬롯, Chain의 기존 정보 우선순위는 변경하지 않는다.

이 변경은 색 면적을 크게 줄이면서도 색 이외의 outline·line·상태점으로 ON/OFF를
구분하므로 공연 가독성과 시각적 정돈을 함께 유지한다.

## 13. 상단 도구와 프리셋 식별 재배치

추가 실물 피드백에서 하단 SET이 독립된 큰 기능처럼 보이고, 왼쪽 상단의 단독 숫자는
프리셋 번호라는 의미가 불명확하다는 문제가 확인됐다.

- SETTINGS는 텍스트 버튼 대신 왼쪽 위 모서리의 톱니바퀴 아이콘으로 바꾼다.
- 프리셋 번호는 항상 두 자리로 표시하고 슬롯과 같은 크기의 `08-B`로 묶는다.
- 톱니바퀴, 프리셋 이름, `08-B`의 시각적 수직 중심을 같은 기준선에 맞춘다.
- 톱니바퀴 glyph는 30px에서 27px로 정확히 10% 줄인다.
- 프리셋 이름은 다시 화면 중심에 맞춘다.
- Master와 BPM은 하단의 동일 높이 2열 Performance Rail로 배치한다.
- 이전 SET 위치는 검증되지 않은 가짜 기능으로 채우지 않는다.

이전 SET 위치의 우선 후보는 Tuner지만, TONEX ONE에서 실제 pitch/note telemetry를
확인하기 전에는 버튼을 노출하지 않는다. 데이터가 검증되면 그 위치를 Tuner 진입점으로
사용하고, 검증 전에는 공연 화면의 시각적 휴식 공간으로 남긴다.

## 14. 하단 최소 Metadata

Master/BPM을 왼쪽에 세로로 쌓은 시안은 오른쪽 절반이 비어 보여 화면의 무게 중심이
왼쪽 아래로 쏠렸다. 다시 2열 Performance Rail로 묶은 시안도 불필요한 선과 구조가
생겨 하단이 별도 패널처럼 보였다. 최종적으로 다음 원칙을 사용한다.

- 선, 구분선, 외곽 카드, 배경 채움, 그림자를 모두 제거한다.
- 480×320에서는 13px의 `VOL -8.5 dB`와 `BPM 118`만 왼쪽 아래에 한 줄로 둔다.
- Rail과 metadata 사이의 공백은 정보 부족이 아니라 공연 가독성을 위한 휴식 공간이다.
- 좁은 세로형 화면에서는 단위를 생략한 `VOL -8.5`로 축약해 잘림을 막는다.
- 나머지 공간은 검증된 실시간 정보나 실제 동작이 생기기 전까지 비워 둔다.

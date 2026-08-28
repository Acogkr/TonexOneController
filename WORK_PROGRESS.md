# 장시간 검증 및 개선 진행 기록

마지막 갱신: 2026-08-10 (Asia/Seoul), Phase 19 자산/펌웨어 공간 최적화 완료

## 현재 작업 목표

- Rust 기반 TONEX ONE 전용 컨트롤러 전체를 기능, 안정성, 성능, 보안,
  UI/UX, 하드웨어 연동, 테스트 품질, 유지보수성 관점에서 재검증한다.
- 현재 하드웨어에서 실제로 사용할 수 있는 튜너 입력 경로를 근거로 결정하고,
  안전하게 가능한 범위까지 구현 및 검증한다.
- 사용자가 이미 만든 변경을 보존하며 검증 → 수정 → 회귀 검증을 반복한다.

## 현재 상태와 기존 변경사항

- 프로젝트 루트에 `.git` 디렉터리가 없어 `git status`, `git diff`, 브랜치
  확인은 불가능하다. 파일을 되돌리거나 삭제하지 않고, 이 문서와 파일별
  수정 시각 및 실제 내용을 기준으로 변경을 추적한다.
- 2026-08-10 최근 변경은 Web UI의 내부 페이지 이동, Android 뒤로가기,
  브라우저 기본 선택 메뉴 제거, 로컬 접속 이름 변경과 관련되어 있다.
- 안정적으로 실물 검증된 JC3248W535 제품 조합은 `wifi-web`과
  `board-jc3248w535`이며, BLE MIDI와 Serial MIDI는 포함하지 않는다.
- 기존 README는 실시간 튜너 pitch 데이터 경로가 아직 검증되지 않았음을
  명시한다.

## 완료한 작업

- 목표 문서 전체 확인
- 프로젝트 루트, README, Cargo workspace, 펌웨어 feature 구성 확인
- 주요 튜너 참조와 기존 경계 문서 위치 확인
- 최근 수정 파일 목록 확인
- 전체 workspace 테스트 재실행
- 포맷, Clippy 경고 금지, Web 자산, 진단 분석기, 하드웨어 검증 도구,
  미사용 의존성, unsafe 경계, 제품 범위, 소스 위생, 취약점, 라이선스,
  의존성 출처 정책 검증
- JC3248W535 사양서와 회로도 시각 검토
- TONEX ONE 설명서의 튜너/USB 오디오 관련 페이지 검토
- 모바일 390x844 viewport에서 Web 메인/설정 화면과 설정 hash route 확인
- 튜너 입력 아키텍처와 pitch algorithm 비교 및 근거 문서 작성
- allocation-free YIN pitch detector와 합성 신호 검증 구현
- 22개 전체 보드의 공통, BLE 전용, Wi-Fi Web+Serial MIDI target matrix 검증
- 4MB 보드를 제외한 20개 보드의 Wi-Fi Web+Serial MIDI+BLE 최대 조합 검증
- Web 상태 게시 경합, captive DNS 실패 격리, 모바일 내부 뒤로가기 감사
- 변경 없는 슬롯/Bluetooth DOM 재생성 제거와 Web snapshot host benchmark
- Tuner UI의 octave, current/target frequency와 상태별 안내 확장
- 같은 음의 meter jitter를 줄이되 새 note는 즉시 반영하는 allocation-free smoothing 추가
- 50개 보드 이미지의 무손실 row-RLE 변환과 caller-owned row decoder 구현
- 보수적인 4MiB factory 기준 Wi-Fi Web 앱 사용량 94.76%에서 76.88%로 감소;
  JC3248W535 실제 large partition 기준 62.10%
- 오래된 README/verification/completion/performance/architecture 문서를 현재 코드와 대조해 수정
- 잘못된 Station Wi-Fi 저장 후 Web 접근을 잃지 않도록 기본 AP recovery 구현

## 수정한 파일

- `WORK_PROGRESS.md`: 장시간 작업의 중단 지점과 증거를 남기기 위해 추가
- `tools/test-hardware-validation-tool.ps1`: 4MB 보드에서 금지된 Wi-Fi+BLE
  조합을 성공 사례로 기대하던 오래된 테스트를 올바른 거부 테스트로 수정
- `rust/crates/tonex-application/src/lib.rs`: 커진 명령 처리 함수에서 Bluetooth와
  풋 컨트롤 설정 처리를 작은 함수로 분리
- `rust/crates/tonex-application/src/runtime.rs`: A/C 슬롯의 동일 분기를 병합
- `rust/crates/tonex-web/src/lib.rs`: 큰 snapshot 직렬화 함수에서 MIDI와
  Bluetooth 장치 직렬화를 분리
- `rust/firmware/tonex-controller/src/main.rs`: 큰 설정값을 참조로 전달하고,
  동일 슬롯 분기를 병합. 자연어 주석이 소스 위생 검사에 걸리지 않도록 표현 정리
- `rust/crates/tonex-web/assets/index.html`: 장치에서 온 preset name을 `innerHTML`로
  삽입하지 않고 text node로 렌더해 DOM injection 방지
- `rust/crates/tonex-web/src/lib.rs`: 위 Web 보안 경계 회귀 검사 추가
- `rust/crates/tonex-tuner/`: `no_std` YIN 검출 코어, synthetic tests와 host
  참고 benchmark 추가
- `Cargo.toml`, `Cargo.lock`: 새 workspace crate와 `libm` 의존성 등록
- `docs/rust-migration/tuner-feasibility-2026-08-10.md`: 하드웨어 근거,
  아키텍처/알고리즘 비교, 구현/테스트 결과와 실물 검증 gate 기록
- `rust/firmware/tonex-controller/src/main.rs`: BLE-only 구성에서도 modem을
  Wi-Fi/Bluetooth typed peripheral로 분할하고 Bluetooth half만 전달하도록 수정.
  command coalescing helper는 Web/MIDI feature 또는 test에서만 컴파일하도록 제한
- `README.md`: 2026-08-10 matrix와 tuner core 결과를 반영하고, 실제 BLE/UAC2
  검증이 아직 남았음을 명시
- `rust/firmware/tonex-controller/src/web.rs`: 풋 컨트롤 snapshot 게시 실패를
  독립적으로 재시도하고, captive DNS 실패가 Web 전체를 중단하지 않도록 격리
- `rust/firmware/tonex-controller/src/main.rs`: 풋 컨트롤 게시 성공 상태를 별도로
  추적하여 Mutex 경합 뒤에도 반드시 다시 게시
- `rust/crates/tonex-web/assets/index.html`: 변경 없는 슬롯/Bluetooth DOM 재생성을
  생략하고, hash 복원 및 중첩 페이지를 포함한 명시적 browser history 구현
- `docs/rust-migration/failure-mode-audit-2026-08-10.md`: 정상/실패/재연결,
  보안/자원 경계와 실물 검증 gate 기록
- `rust/crates/tonex-ui-model/src/lib.rs`: 모순 가능한 tuner bool 조합을
  `Unavailable/Waiting/Weak/Stable` 상태 머신으로 교체하고 주파수 필드 추가
- `rust/crates/tonex-ui-renderer/src/lib.rs`: octave, current/target frequency,
  weak/waiting/unavailable 안내와 상태별 meter 회귀 테스트 추가
- `rust/crates/tonex-tuner/src/lib.rs`: 같은 note EMA, 즉시 note 전환, bounded dropout
  hold를 제공하는 allocation-free `PitchSmoother`와 테스트 추가
- `rust/crates/tonex-ui-renderer/build.rs`: 1.92MB raw RGB565 blob 대신 build-time
  lossless row-RLE blob과 row offset table 생성
- `rust/crates/tonex-ui-renderer/src/lib.rs`: 480-byte caller-owned row buffer로
  무할당 decode하고 source row가 바뀔 때만 복원; 모든 4,000개 row 회귀 검사 추가
- `rust/firmware/tonex-controller/src/web.rs`: pedal BMP fallback도 압축 row를
  순차 decode해 전송하도록 변경
- `README.md`, `docs/rust-migration/{verification,completion-audit,performance,architecture}.md`:
  schema v5, BLE Central, 지원 matrix, 현재 Web 주기/버퍼/성능/크기 증거로 갱신
- `rust/firmware/tonex-controller/src/web.rs`, `main.rs`: Station 연결 또는 netif
  준비 실패 시 기본 WPA2 AP로 전환하고 runtime/QR/Web에 실제 fallback 상태 게시

## 발견한 문제

- 프로젝트 루트의 Git 메타데이터가 없어서 기존 변경과 이번 변경을 Git으로
  정확히 구분할 수 없다.
- 현재 튜너는 화면 모델과 렌더러만 존재하며 실제 pitch 입력은 연결되지 않았다.
- README의 2026-07-31 검증 결과는 현재 2026-08-10 코드에 대한 충분한 증거가
  아니므로 전체 테스트를 다시 실행해야 한다.
- 하드웨어 검증 도구 테스트가 4MB 보드에서 명시적으로 금지된 Wi-Fi+BLE
  조합을 성공으로 기대해 전체 검증을 중단했다.
- 최근 변경으로 여러 함수가 Clippy의 크기/중복 분기 검사를 통과하지 못했다.
- JC3248W535에는 240MHz 듀얼코어 ESP32-S3, 512KB SRAM, 8MB PSRAM,
  16MB Flash와 I2S 스피커 출력은 있지만 마이크/기타 입력용 codec 또는
  아날로그 입력 회로가 없다. 확장 GPIO 중 일부는 ADC 가능하지만 기타 신호를
  직접 안전하게 받을 입력 버퍼/바이어스/보호 회로가 없다.
- TONEX ONE은 24-bit/44.1kHz USB 오디오 인터페이스이며 로컬 C 자료가 장치를
  UAC 2.0 복합 장치로 식별한다. 현재 펌웨어는 같은 장치의 CDC interface 0만 연다.
- Espressif의 기존 `usb_stream` 호스트 드라이버는 UAC 1.0만 지원하므로 TONEX
  ONE 오디오를 그대로 연결하는 안전한 즉시 경로가 아니다.
- Web preset 목록이 장치 제공 preset name을 `innerHTML`로 삽입해 특수 문자열이
  DOM 구조로 해석될 수 있었다.
- 휴대폰 Web microphone은 secure context와 사용자 권한이 필수다. 현재 AP의
  HTTP 주소는 일반 모바일 브라우저에서 secure context가 아니므로 기본 tuner
  입력으로 사용할 수 없다.
- BLE-only target build가 일반 `Modem`을 `BluetoothModem` 자리로 전달해 첫 보드에서
  컴파일되지 않았다. 기본 Wi-Fi 제품 검증만으로는 발견되지 않던 feature 조합 결함이다.
- radio/MIDI가 없는 보드 target에서 Web/MIDI command coalescing helper가 dead-code
  경고를 반복했다.
- Web snapshot의 UI와 parameter 게시가 성공하고 풋 설정 Mutex만 경합하면,
  풋 설정이 오래된 상태로 남아도 다음 게시가 예약되지 않았다.
- captive DNS task 시작 실패가 숫자 IP로 가능한 Web 제어까지 중단시켰다.
- Web이 같은 상태를 200ms마다 받을 때 슬롯과 Bluetooth 목록 DOM을 불필요하게
  매번 다시 만들었다.
- hash 페이지를 직접 복원한 모바일 browser에는 메인 이력 항목이 없어 첫
  hardware Back이 Web 자체를 닫을 수 있었다.
- Tuner UI가 octave와 current/target frequency를 표시할 상태가 없었고,
  `TONEX ONE REQUIRED` 문구는 TONEX가 연결된 입력 미지원 상황을 잘못 설명했다.
- Tuner detector 출력에 meter jitter와 한 frame dropout을 다루는 평활 계층이 없었다.
- 4MiB 호환 factory 기준 Wi-Fi Web 앱이 94.76%를 사용했으며, 원인은
  50개 240×80 board skin을 raw RGB565 1.92MB로 중복 보관한 구조였다.
- 주요 검증 문서에 schema v2/v3, BLE peripheral, 100ms Web polling,
  22-board maximum feature 같은 현재와 다른 설명이 남아 있었다.
- 잘못되거나 사라진 Station 네트워크를 저장하면 다음 부팅에서 Web start가 실패해
  사용자가 Wi-Fi 설정을 되돌릴 접근 경로가 사라졌다.

## 해결한 문제

- 오래된 하드웨어 검증 도구 테스트의 모순을 수정했다.
- 새 코드의 Clippy 중복 분기, 큰 값 복사, 과도하게 큰 함수 문제를 동작 변경 없이
  정리했다.
- 소스 위생 검사의 자연어 주석 오탐을 해당 주석의 명확한 표현으로 해소했다.
- preset name Web DOM injection 경로를 text-only rendering으로 수정했다.
- 실제 입력 adapter와 분리된 pitch detection 코어를 구현해 향후 UAC2/ADC 경로가
  검증될 때 기존 UI에 안전하게 연결할 수 있게 했다.
- BLE-only modem을 명시적으로 split해 모든 보드에서 올바른 Bluetooth peripheral
  타입을 소유하도록 수정했다.
- feature 없는 제품 build에서 불필요한 command batching code와 경고를 제거했다.
- 풋 설정 게시 결과를 독립적으로 추적해 경합 시 다음 main loop에서 재시도한다.
- captive DNS는 선택 기능으로 격리하고 실패 시 `192.168.4.1` Web을 유지한다.
- 변하지 않은 슬롯/Bluetooth DOM은 재사용해 통신 반응성을 유지하면서 모바일
  렌더링 부하를 줄였다.
- 메인 route를 browser history 기준점으로 만들고 내부 페이지마다 history entry를
  추가해 중첩 페이지와 직접 복원 모두에서 뒤로가기가 먼저 Web 내부를 닫게 했다.
- Tuner 신호 상태를 enum으로 제한하고 note+octave, frequency, target, cents와
  정확/flat/sharp/weak/waiting/unavailable 상태를 화면에 구분했다.
- 같은 note만 평활하고 다른 note는 즉시 교체하며 dropout hold가 끝나면 신호 상태를
  명확히 낮추는 추적기를 구현했다.
- board skin을 row별 RLE로 압축하고 480-byte stack row에 무할당 decode해
  Web/board 시각 결과를 유지하면서 최종 앱 이미지에서 738,432B를 절감했다.
- 현재 코드와 문서를 다시 대조해 과거 수치와 지원하지 않는 조합 주장을 수정했다.
- Station 연결 실패를 radio 전체 실패로 전파하지 않고 알려진 `TONEX-ONE` 기본 AP로
  복구하며, 저장값이 아닌 실제 동작 중인 AP 정보를 UI snapshot에 사용한다.

## 실행한 테스트와 결과

- `cargo test --workspace --quiet`: 통과
- `tools/verify-rust-migration.ps1`: 최종 통과
  - Rust formatting: 통과
  - 생성된 TONEX 파라미터 registry: 통과
  - Embedded Web UI: 통과
  - 진단 분석기 positive/negative fixture: 통과
  - 22-board 하드웨어 검증 도구 fixture: 통과
  - Clippy 전체 workspace/all-targets, 경고 금지: 통과
  - 전체 Rust tests/doc-tests: 통과
  - cargo machete: 미사용 직접 의존성 없음
  - unsafe 경계 및 TONEX ONE-only 제품 범위: 통과
  - cargo audit: 알려진 취약점 없음
  - cargo deny: advisories/bans/licenses/sources 통과
- `cargo test -p tonex-tuner`: 최종 8개 합성 신호·smoothing test 통과
  - 표준 기타 E2/A2/D3/G3/B3/E4와 A4: 1 cent 이내
  - -17/+23 cents: 1 cent 이내
  - harmonic+noise: 2 cents 이내
  - silence/비주기 noise 거부
  - B1에서 E4로 frame 전환
- `cargo run --release -p tonex-tuner --example benchmark_detector`:
  x86-64 host에서 2048 sample frame 평균 488us (ESP32-S3 실측 아님)
- JC3248W535 `wifi-web,board-jc3248w535` release target build: 통과
- 새 tuner/Web 변경 후 `tools/verify-rust-migration.ps1` 재실행: 통과
- 22-board 공통 ESP32-S3 release matrix: 전부 통과
- 22-board BLE-only ESP32-S3 release matrix: 최초 type 오류 재현, 수정 후 전부 통과
- 22-board Wi-Fi Web+Serial MIDI release matrix: 전부 통과
- 20-board(8/16MB) Wi-Fi Web+Serial MIDI+BLE 최대 조합 matrix: 전부 통과
- 4MB `waveshare-zero`, `pirate-polar-zero`의 Wi-Fi+BLE 동시 조합은 의도적으로
  지원하지 않으며 하드웨어 검증 도구의 거부 test가 통과한다.
- BLE modem 수정과 모든 matrix 완료 후 `tools/verify-rust-migration.ps1` 최종
  회귀 실행: 통과
- 의존성 중복 버전 경고는 존재하지만 현재 정책 위반이나 직접 미사용 의존성은 아니다.
- Web snapshot host benchmark: 590 bytes, 20,000회 median 0.9us, p95 1.1us
  (ESP32-S3 실측 아님)
- 자동 browser: 메인 → Settings → Bluetooth → Settings → 메인 순서 통과
- 자동 browser: `#settingsSheet` 직접 진입 뒤 첫 Back이 메인으로 복귀
- Phase 17 변경 후 `tools/verify-rust-migration.ps1` 전체 회귀: 통과
- Phase 17 변경 후 JC3248W535 `wifi-web,board-jc3248w535` ESP32-S3 release build: 통과
- Tuner UI model/application/renderer tests: 통과, 최종 10개 renderer tests
- Tuner detector+smoother synthetic tests: 8개 통과
- medium/portrait/compact Tuner BMP 미리보기 시각 검사: 겹침 수정 후 통과
- 압축된 50 skins × 80 rows = 4,000 rows exact-width decode test: 통과
- 압축 전/후 전체 67 UI BMP SHA-256 비교: 불일치 0
- host UI benchmark: Tiny 38us, Compact 112us, Portrait 146us,
  Landscape 135us, Medium 228us, Large 733us median
- Wi-Fi Web app image: 3,912,624/4,128,768B (94.76%)에서
  3,174,192/4,128,768B (76.88%)로 감소; JC3248W535 실제
  5,111,808B large partition에서는 62.10%, 1,937,616B 여유
- row-RLE 변경 시점 전체 verification gate: 통과, 당시 workspace test 총 177개
- row-RLE 변경 후 22-board Wi-Fi Web ESP32-S3 release matrix: 전부 통과
- Station fallback 변경 후 JC3248W535 Wi-Fi Web 및 Wi-Fi 없는 core release build: 통과
- Station fallback 변경 후 전체 verification gate: 통과

## 실제 하드웨어에서 확인한 내용

- 이 작업 시작 전 사용자 확인: 최신 안정 펌웨어에서 화면, TONEX ONE 연결,
  preset volume 동기화는 정상 동작했다.
- 이번 장시간 검증 사이클에서는 아직 보드에 새 펌웨어를 올리지 않았다.

## 확인하지 못한 내용

- 현재 소스 전체 빌드 및 테스트 결과
- 실제 보드 재부팅, Wi-Fi 재연결, 브라우저 호환성
- BLE MIDI 실물 안정성
- 실제 pitch 입력 경로 및 튜너 정확도
- Android/Samsung Internet/Chrome 실기기의 hardware Back과 custom select 동작

## BLOCKED 항목

- Git 이력 부재는 기록 정밀도를 낮추지만 작업 진행을 막지는 않는다.
- TONEX ONE의 UAC 2.0 audio streaming descriptor와 실제 isochronous endpoint를
  현재 보유한 CDC 로그만으로 확정할 수 없다. 실물 USB descriptor capture가 필요하다.
- Browser 자동화에서 설정 hash route 진입은 확인했지만 연속 hardware Back 자동
  검증은 제어 시간 초과로 완료하지 못했다. 정적 route test는 통과했다.

## 아직 남은 작업

- 정상/실패/재연결/상태 복원 시나리오 실물 검증
- Web UI 실제 Android·Samsung Internet·Chrome 호환성 검증
- 실제 보드의 성능, 메모리, 네트워크 장시간 측정
- 실제 Android/Samsung Internet/Chrome 및 보드 WebSocket 검증
- 실제 TONEX ONE UAC2 descriptor capture와 CDC 동시 소유 가능성 검증
- 실제 MCU에서 tuner CPU/heap/latency 측정 후 adapter 연결 여부 결정

## 다음 작업

- Web/통신/복구/보안 구현을 정적으로 감사하고 부족한 실패 테스트를 추가한다.
- 정상/실패/재연결/상태 복원 test coverage를 다시 대조한다.
- 실물 보드가 의도적으로 boot mode에 놓였을 때만 최신 안정 조합을 flash하고,
  화면/터치/TONEX/Wi-Fi 재연결을 검증한다.
- 실제 입력이 없는 상태를 live tuner로 표시하지 않는다.

## Phase 20 — 제품 자산 독립화와 검증 패키지 재검증 (2026-08-10)

- 보드용 앰프 스킨 50개를 `legacy/skins_png`에서
  `rust/crates/tonex-ui-renderer/assets/skins`로 이동했다. 현재 제품 빌드는
  과거 C 프로젝트의 스킨 폴더에 의존하지 않는다.
- `build-rust-esp.ps1`가 4MiB 보드의 Wi-Fi+BLE 동시 조합을 명시적으로 거부하도록
  만들고, 전체 보드 검사와 `-TargetMatrix`가 지원 조합 4종을 정확히 검사하도록
  정리했다.
- 하드웨어 검증 도구 회귀 테스트, UI renderer 10개 테스트, 전체 Rust 검증 gate를
  다시 실행했고 모두 통과했다.
- `tonex-one-rust-verification-20260810-075900.zip`을 새로 만들었다. 인접 SHA-256이
  일치했고, 작업 폴더 밖에 압축을 풀어 995개 파일의 무결성 검사와 패키지 자체의
  전체 host verifier를 실행했으며 모두 통과했다.
- 현재 패키지는 이 컴퓨터에서 독립적으로 재현됨을 확인했지만, 다른 컴퓨터의
  독립 감사와 실물 하드웨어 검증은 아직 완료로 주장하지 않는다.
- 수정된 보드 행렬 도구로 네 조합을 다시 검증했다. core 22/22,
  Bluetooth-only 22/22, Wi-Fi Web+Serial MIDI 22/22, 최대 기능 20/20이
  통과했고, 최대 기능에서 4MiB 보드 2개는 명시된 정책대로 건너뛰었다.
- 네 행렬을 순차 실행하는 단일 명령은 이 실행 환경의 30분 제한에 걸렸지만,
  서로 다른 target 폴더에서 조합별로 병렬 실행해 동일 범위를 완료했다.
- 문서 기록까지 포함한 검증 패키지를 생성하고, 인접 SHA-256과 별도 임시
  폴더의 995개 파일 무결성을 확인했다. 마지막 기록 갱신 뒤 패키지를 한 번 더
  생성해 최신 산출물로 교체한다.

## Phase 21 — 처음 사용하는 모바일 사용자 관점 재검증 (2026-08-10)

- 실제 브라우저를 360×800 모바일 viewport로 설정해 메인, Settings,
  Bluetooth 중첩 페이지를 조작했다.
- Settings 진입 후 브라우저 뒤로가기는 메인으로, Bluetooth에서 첫 뒤로가기는
  Settings로, 두 번째 뒤로가기는 메인으로 정확히 복귀했다.
- 360px 폭에서 가로 넘침은 0px이었고, 브라우저 기본 `<select>`는 화면에
  노출되지 않았으며 제품 스타일의 선택 패널만 사용됐다.
- 설정 톱니바퀴, 프리셋 키, 파라미터 증감, 풋 컨트롤 모드, 공통 Back 버튼의
  최소 터치 높이를 44px로 통일했다.
- 수정 후 모바일 재검증에서 Settings와 Back은 각각 44px, 가로 넘침 0px,
  노출된 native select 0개, 콘솔 오류 0개였다.
- Web 자산 검사, `tonex-web` 17개 테스트, 전체 migration verifier,
  JC3248W535 Wi-Fi Web+Bluetooth release build가 모두 통과했다.
- 이 브라우저 검증은 Chromium 기반 로컬 미리보기 결과이며, 실제 Samsung
  Internet과 보드가 제공하는 WebSocket 연결은 여전히 실물 검증 대상이다.
- Windows에서 ESP32-S3 USB JTAG/Serial 장치와 COM4가 정상 연결된 것은
  읽기 전용으로 확인했다. VID/PID만으로 일반 실행과 부트로더 상태를 확정할 수
  없어 플래시는 수행하지 않았다.
- 사용자 실제 모바일 브라우저는 Samsung Internet이 아니라 Chrome이다. 이후
  실물 Web UI 검증은 Android Chrome을 우선하고, Samsung Internet은 목표 문서의
  추가 호환성 확인 항목으로만 유지한다.
- 최신 JC3248W535 Wi-Fi Web+Bluetooth 빌드 산출물은
  `C:\tx-final\xtensa-esp32s3-espidf\release\tonex-controller`이며 크기는
  3,730,264바이트다. 이는 링크 산출물 확인이며 실물 동작 증거는 아니다.
- 최신 검증 ZIP을 원본 작업 폴더 밖에 풀고 그 패키지 자체의 전체 host
  verifier를 다시 실행했으며 통과했다.
- 같은 추출 패키지를 유일한 source root로 사용해 JC3248W535 Wi-Fi
  Web+Bluetooth release target build를 완료했다. 링크 산출물은 3,730,264바이트로
  작업 폴더 빌드와 크기가 정확히 같았다.
- 위 결과는 현재 컴퓨터에서 패키지가 자체 완결적임을 증명하지만, 별도 컴퓨터의
  독립 감사나 실물 보드 동작을 대신하지 않는다.
- COM4에 연결된 JC3248W535를 대상으로 `-PlanOnly` 하드웨어 검증 계획을 다시
  생성했다. 선택 결과는 `board-jc3248w535,wifi-web,ble-midi`, 16MiB flash,
  `partitions.large.csv`였고 실제 build/flash/monitor는 수행하지 않았다.

## 최신 실물 플래시 — 2026-08-10

- 사용자가 JC3248W535를 부트 모드로 연결한 뒤 COM4, ESP32-S3 revision v0.2,
  16MiB flash를 확인했다.
- `board-jc3248w535,wifi-web,ble-midi`와 `partitions.large.csv` 조합을 COM4에
  정상 플래시했다.
- 플래시된 앱 크기는 3,610,016/5,111,808바이트(70.62%)였고 도구 오류 없이
  완료됐다.
- 플래시 후 사용자가 TONEX ONE을 보드 USB Host에 연결했지만 보드가 장치를
  연결됨으로 전환하지 못했다고 보고했다. 따라서 이 Wi-Fi Web+BLE 조합은 실물
  TONEX 연결 검증에 실패한 상태이며 안정 이미지로 취급하지 않는다.
- Bluetooth 런타임은 TONEX 준비 이후로 지연되어 있으므로, 다음 검증은 동일
  최신 코드의 Wi-Fi Web 단독 빌드로 USB 연결을 먼저 복구하고 기능 조합에 따른
  회귀인지 분리한다.

## TONEX USB 회귀 원인 및 통합 수정 — 2026-08-10

- Wi-Fi 단독과 Wi-Fi+BLE ELF를 비교했다. BLE 조합은 IRAM 약 14.7KiB,
  internal DRAM data/BSS 약 2.3KiB가 증가해 내부 heap 시작점이 약 17KiB
  높아졌다.
- 보존된 C 펌웨어를 다시 대조해 `TONEX_RX_TEMP_BUFFER_SIZE(8192) +
  TONEX_USB_TX_BUFFER_SIZE(512) + 256 = 8960`바이트의 DMA 메모리를 부팅 시
  선점하고 CDC open 직전에 해제한 뒤 disconnect 후 다시 선점하는 중요한
  안정화 경로가 Rust 이식에서 빠진 것을 확인했다.
- `esp-idf-usb-host::TonexUsbStack`에 8,960바이트 DMA reserve를 RAII 소유자로
  구현했다. install 시 예약 실패는 즉시 오류, open 직전 해제, open/config 실패와
  close 후 재예약을 보장한다.
- USB host 단위 테스트는 8개로 증가했고 reserve 크기 불변식을 검사한다.
- 현재 workspace 전체 테스트는 179개이며 전체 verification gate가 통과했다.
- 전체 migration verifier와 JC3248W535 Wi-Fi Web+BLE optimized target build가
  통과했다. 수정 전후 ELF 정적 DRAM/파일 크기는 동일해 reserve가 런타임 heap
  보장만 추가했음을 확인했다.
- `docs/rust-migration/physical-regression-ledger.md`를 추가해 실물 성공, 실패,
  후보를 분리하고 동일 바이너리 13개 회귀 항목을 모두 통과하기 전에는 `final`
  또는 안정 이미지로 부르지 않도록 고정했다.
- 직접 `espflash save-image`를 호출하면 feature는 BLE인데 sdkconfig에 Bluetooth가
  빠지는 별도 도구 불일치를 발견했다. 실패한 산출물은 생성되지 않았고,
  `build-rust-esp.ps1 -OutputImage`가 일반 build와 동일한 sdkconfig·feature·flash
  size·partition 정보를 사용해 병합 이미지를 만들도록 통합했다.
- 고정 후보는 `artifacts/jc3248w535-wifi-ble-dma-reserve-candidate.bin`, 16MiB,
  앱 3,610,560/5,111,808바이트(70.63%), SHA-256
  `3d7a9eb0151469a404e60ed7e5224d0fa169468b895239c691b72c555cc0ec74`이다.
- `tools/flash-locked-image.ps1`를 추가했다. 16MiB 크기와 SHA-256을 모두 확인한
  뒤에만 고정 병합 이미지를 0x0부터 기록하며, 잘못된 checksum 거부와 PlanOnly
  정상 경로를 검증했다. 이후 실물 회귀에서는 다시 빌드하지 않고 이 후보 하나만
  사용한다.
- 첫 DMA 후보를 다시 감사해 TONEX가 존재하지 않는 동안의 timed CDC open이
  예약을 1초씩 해제할 수 있는 경쟁 조건을 발견했다. 첫 후보 SHA
  `3d7a...ec74`는 superseded로 지정하고 플래시 금지했다.
- `open_tonex_one`과 main loop 모두 exact TONEX VID/PID descriptor가 관찰된
  경우에만 open을 시도하도록 보강했다. 장치가 없을 때 retry delay도 0으로
  유지해 실제 연결 시 즉시 열며, 그전까지 DMA reserve는 계속 유지된다.
- 현재 고정 후보는 `jc3248w535-wifi-ble-dma-reserve-candidate-v2.bin`, 16MiB,
  앱 3,610,608/5,111,808바이트(70.63%), SHA-256
  `094107d5ce7dac4dbf6a27a1e0c8dfaabaad8e56a4ed01c24c85c8544e36355f`이다.
- v2 후보 생성 뒤 locked-image 크기/checksum PlanOnly 검사와 전체 migration
  verifier를 다시 실행해 모두 통과했다. 실물에는 아직 v2를 기록하지 않았다.
- 레거시 USB init을 끝까지 재검토해 CDC open 후 100ms, DTR/RTS 및 line coding
  설정 후 250ms를 기다린 다음 handshake를 시작하는 안정화 지연도 Rust에서
  빠졌음을 확인했다. v2는 superseded로 지정해 플래시 금지했다.
- `TonexUsbStack::open_tonex_one`에 동일한 100ms/250ms 지연을 복원하고 device
  handle을 마지막 지연 뒤에만 게시해 application handshake가 앞서지 못하게 했다.
  USB host 테스트는 9개로 증가했다.
- 후속 재연결 감사에서 disconnect 직후 DMA reserve 재확보 실패를 호출자가
  무시하면 다음 open이 reserve 없이 진행될 수 있는 경로를 발견했다. 모든
  open/reopen 직전에 전체 8,960바이트 reserve를 다시 보장하도록 수정했고 v3는
  superseded로 지정해 플래시 금지했다.
- 현재 고정 후보는 `jc3248w535-wifi-ble-usb-stable-candidate-v4.bin`, 16MiB,
  앱 3,610,688/5,111,808바이트(70.63%), SHA-256
  `489e2b3300306857d7e770d8537f5790cc7ecf7009e61a3fa4efe7fd41b80838`이다.
  locked-image plan, 179개 workspace tests, full migration verifier가 통과했다.
- 2026-08-10 18:20 KST에 위 체크섬의 정확한 16MiB v4 이미지를 COM4의
  JC3248W535에 재빌드 없이 플래시했다. 플래시 자체는 성공했으며 실물 회귀
  항목은 사용자 관찰 전까지 통과로 승격하지 않는다.
- v4 플래시 후 사용자가 TONEX ONE 미연결을 보고해 v4를 실물 실패로 확정했다.
  다른 회귀 항목을 계속 요구하지 않고 USB 초기화만 레거시 C와 다시 대조했다.
- 레거시의 `vTaskDelay(500)`은 현재 100Hz FreeRTOS 설정에서 5초인데 Rust USB
  Host 설치 전에는 빠져 있었다. DMA reserve를 먼저 확보한 채 Host 시작을 5초
  늦추고, CDC open 후 line-coding GET → SET → DTR/RTS 순서도 그대로 복원했다.
- 형식 gate 전 만들어진 v5는 이름상 superseded 처리했다. 형식 수정 전후 병합
  바이너리가 동일한 SHA-256을 갖는 것도 확인했고, 검증된 현재 후보는
  `jc3248w535-wifi-ble-usb-stable-candidate-v6.bin`, 앱
  3,610,912/5,111,808바이트(70.64%), SHA-256
  `03852c36fa4ada6e222f2a80bac3aaca252b9161d0dfbe93000d28217edcf8ac`이다.
  locked-image plan과 179개 전체 검증이 통과했다.
- 2026-08-10 18:30 KST에 위 체크섬의 정확한 v6 16MiB 이미지를 COM4에
  재빌드 없이 플래시했고 성공했다. 이제 같은 바이너리의 냉부팅 TONEX 연결
  관찰만 기다리며, 결과 전에는 USB 통과로 기록하지 않는다.
- v6에서도 10초 이상 냉부팅 대기 후 TONEX 연결과 preset 표시가 모두 실패했다.
  따라서 5초 Host 지연과 CDC GET/SET 순서만으로는 원인이 해결되지 않았으며,
  v6도 실물 실패로 고정했다. 이후에는 다른 기능을 시험하지 않고 USB 단계별
  직접 계측과 Wi-Fi 단독/최대 기능 구성 차이 분석으로 전환한다.
- 제품 기능 조합을 축소하지 않은 `usb-connection-diagnostics` feature를 추가했다.
  메인 preset 위치가 `USB HOST STARTING - WAIT 5S`, `USB WAITING FOR TONEX`,
  `USB OPEN ERR <code>`, `SYNCING TONEX ONE`, `USB HANDSHAKE ERR`,
  `USB PROTOCOL ERR` 중 실제 마지막 단계를 표시한다. 일반 제품 build에서는 이
  진단 문구가 비활성화된다.
- 진단 이미지 `jc3248w535-wifi-ble-usb-connection-diagnostic-v1.bin`은 16MiB,
  앱 3,611,200/5,111,808바이트(70.64%), SHA-256
  `4beb9db1d1f2b675cc6a780b6b89cf2974a55908fb98c8e296a1c2aa70991af6`이다.
  전체 verifier와 locked checksum 검증 후 18:46 KST에 COM4로 플래시했다.
- 실물 화면은 `USB OPEN ERR 257`을 표시했다. `257 == ESP_ERR_NO_MEM`이므로
  VBUS, USB enumeration, TONEX VID/PID 식별은 통과했고 CDC transfer allocation만
  실패했음을 직접 확정했다.
- ESP-IDF 5.5의 CDC open은 8,192/512바이트 데이터 전에 notification/control
  DMA buffers, cache alignment, URB metadata와 synchronization objects를 할당한다.
  기존 C의 256바이트 margin을 포함한 8,960바이트 arena는 최대 기능 조합에서
  8,192바이트 연속 tail을 남기지 못했다. 전체 arena를 12KiB로 확대했다.
- 수정 진단판 v2는 앱 3,611,200/5,111,808바이트(70.64%), SHA-256
  `da2f0ab3296e5c4e3768d4835e683d71d8983e1500201423fb391d7c191e147a`이며
  전체 verifier와 locked checksum 검증을 통과했다. 플래시 직전 COM4가 사라져
  아직 실물에는 기록하지 않았다.
- 사용자가 보드를 다시 부트 모드로 연결했고, 2026-08-10 19:00 KST에 위 SHA의
  진단 v2를 COM4에 재빌드 없이 플래시했다. 기록은 성공했으며 TONEX 관찰은
  아직 결과 전이다.
- 진단 v2도 동일한 `USB OPEN ERR 257`로 실패했다. 총 예약량만 늘려서는 reserve
  해제 직후 radio/CDC allocation 경쟁을 제거하지 못함을 확인했다.
- Rust transport에는 32KiB callback ring과 최대 16KiB frame을 분할 처리하는
  streaming decoder가 있으므로 단일 CDC IN transfer가 C와 같은 8KiB일 필요가
  없다. IN transfer를 4KiB로 줄이고 12KiB arena는 유지했다. 링은 정확히 4KiB
  callback 8개를 수용하며 application partial-frame tests가 통과한다.
- 진단 v3는 앱 3,611,200/5,111,808바이트(70.64%), SHA-256
  `353ae72bd41a5dba90223fdbeee3272ff1e6b4b7e198265de372e9ea756e9993`이며
  전체 179개 검증, target build, locked checksum이 통과했다. 실물 플래시는
  다음 COM4 부트 연결을 기다린다.
- 2026-08-10 19:09 KST에 위 SHA의 진단 v3 16MiB 이미지를 COM4에 재빌드 없이
  플래시했고 성공했다. 이제 동일 바이너리에서 TONEX open 결과만 기다린다.
- v3에서 사용자가 TONEX를 여러 번 조작한 뒤 한 번 실제 연결에 성공했으나 이후
  다시 무반응 상태가 되었다. 4KiB CDC transfer가 open/sync 가능한 것은 확인됐지만
  attach/reconnect 이벤트 신뢰성은 실패로 남겼다.
- 장치가 8초 동안 감지되지 않으면 USB root port를 250ms 자동 power-cycle하고
  다시 enumeration하는 복구 경로를 추가했다. 열린 CDC에는 절대 적용하지 않으며,
  사용자가 pedal/케이블을 반복 조작할 필요를 없애는 목적이다.
- 첫 자동복구 산출물 v4는 표시되지 못하는 중간 진단 assignment 경고가 있어
  플래시 전에 superseded 처리했다. 경고를 제거한 v5는 앱
  3,611,728/5,111,808바이트(70.65%), SHA-256
  `33493cc1487d639d981ec4ad8dd25c599fc387c77db2292065194bc9ca2200a4`이며
  target build, 179개 전체 verifier, locked checksum이 모두 통과했다.
- 2026-08-10 19:20 KST에 위 SHA의 자동복구 진단 v5를 COM4에 재빌드 없이
  플래시했고 성공했다. 이제 pedal을 누르거나 cable을 반복 조작하지 않은 상태에서
  최대 20초 내 자동 enumeration/sync 여부를 확인한다.
- v5에서 `USB HOST STARTING`이 반복 재등장하고 오른쪽 edge corruption이 발생했다.
  이는 UI rerender가 아니라 main 전체 재시작의 직접 증거이며, root-port power cycle의
  TONEX inrush가 board reset/brownout을 유발한 것으로 판정했다. v5는 즉시 실패 처리했고
  runtime periodic port power cycling을 전부 제거했다.
- 새 초기화는 USB Host를 `root_port_unpowered=true`로 설치한 뒤 CDC callback 등록과
  Host event daemon 준비가 끝난 시점에 root port를 단 한 번만 power-on한다. 이미
  연결된 TONEX가 client 등록보다 먼저 attach되는 race를 없애면서 반복 inrush는 없다.
- 진단 v6는 앱 3,611,520/5,111,808바이트(70.65%), SHA-256
  `e166e9903ef6baddb21d954b90ca963e9fada3895bf9d6f62faae2d355b85ac8`이며
  warning-free target build, 179개 전체 verifier, unsafe audit와 locked checksum을
  모두 통과했다. 실물 플래시는 다음 COM4 부트 연결을 기다린다.
- 2026-08-10 19:35 KST에 위 SHA의 진단 v6를 COM4에 재빌드 없이 플래시했고
  성공했다. 이제 주기적 VBUS 변경 없이 단일 초기 attach와 화면 안정성을 함께
  관찰한다.
- 사용자는 진단 v6에서 TONEX 버튼을 건드리지 않은 채 정상 연결과 preset 표시를
  확인했다. 따라서 4KiB transfer + 12KiB reserve + CDC/daemon 준비 뒤 단일 root-port
  power-on 조합이 cold attach를 복구한 직접 실물 증거를 확보했다. 아직 같은
  바이너리의 disconnect/reconnect와 진단 feature를 제거한 제품 이미지는 미검증이다.
- 진단 문구를 끈 첫 제품 산출물 v7에서 feature-off unused-variable 경고가 나와
  플래시 금지했다. 경고 수정 전후 제품 병합 바이너리는 동일 SHA였지만 이름상
  구분해 v8만 현재 후보로 삼는다.
- 제품 후보 v8은 앱 3,611,216/5,111,808바이트(70.64%), SHA-256
  `a77370e171692095c0c5dcce09b21933648a9d7f725c8728a4701438d7b1dfc6`이며
  warning-free target build, 179개 전체 verifier, unsafe audit와 locked checksum을
  모두 통과했다. 진단 v6 reconnect 실물 결과 전에는 플래시하지 않는다.

## Phase 24: USB 재연결 경로 교정

- 진단 v6은 최초 연결과 프리셋 동기화에는 성공했지만, 같은 실행 중 TONEX를
  분리했다 다시 연결했을 때 복구되지 않았다. 따라서 제품 후보 v8도 폐기했다.
- 최초 설치의 12KiB 연속 DMA 예약은 그대로 필수로 유지한다. 다만 Wi-Fi와
  Bluetooth가 시작된 뒤에는 12KiB 단일 블록 재확보 실패가 CDC 재연결 자체를
  막거나 실제 USB 오류를 덮지 않도록 사후 예약 복원을 best-effort로 바꿨다.
- 한 번이라도 연결에 성공한 실행에서는 빠른 재삽입 알림을 놓쳐도 250ms의 제한된
  CDC 직접 탐색을 수행한다. 화면 재시작과 보드 전압 문제를 만들었던 root-port
  자동 전원 반복 제어는 다시 넣지 않았다.
- USB host 테스트는 10개로 늘었으며 최초 연결/재연결 결정표를 검증한다. 진단 v7은
  16MiB, 앱 3,611,280/5,111,808바이트(70.65%), SHA-256
  `7e86f8397327b81affd521ee5638a401a3a2f68da30bb4dd7efb194bc747c225`다.
- 2026-08-10 19:51 KST에 위 SHA의 진단 v7 16MiB 이미지를 COM4로 재빌드 없이
  플래시했다. ESP32-S3 revision v0.2, 16MB flash 기록이 성공했으며 부트 모드 해제
  뒤 최초 연결과 같은 실행의 분리/재연결 결과만 기다린다.
- 진단 v7은 최초 연결에는 성공했지만 사용자가 느리다고 관찰했으며, 분리 후 재삽입은
  다시 실패했다. 최초 약 5초는 레거시의 cold-start 안정화 지연을 그대로 둔 결과지만
  재연결 실패는 정상 동작이 아니므로 v7을 폐기했다.
- 레거시 C는 disconnect 때 장치만 닫지 않고 CDC host driver를 uninstall한 뒤 다음
  연결에서 다시 install한다. Rust v7까지는 CDC driver가 영구 설치되어 있어 이 핵심
  수명주기가 달랐다. v8은 장치 close, CDC uninstall, 100ms Host 정리 창, CDC 재설치와
  callback 재등록을 명시적으로 소유한다. root-port 전원 반복 제어는 없다.
- 진단 v8은 앱 3,611,872/5,111,808바이트(70.66%), SHA-256
  `67a1d14cf205fa7a70deaade84a601d295d453bce3575b549a487520722c5c09`이며
  warning-free 대상 빌드와 전체 migration verifier를 통과했다.
- 2026-08-10 20:06 KST에 위 SHA의 진단 v8 16MiB 이미지를 COM4로 재빌드 없이
  플래시했다. 기록된 다음 실물 gate는 reset 후 최초 연결과 동일 실행의 unplug/replug다.
- 진단 v8에서는 사용자가 보드와 TONEX ONE을 연결해도 아무 연결 상태가 나타나지
  않는 최초 연결 회귀를 확인했다. CDC driver uninstall/reinstall 변경은 즉시 전부
  되돌렸고 v8을 영구 실패 처리했다. 소스와 다음 롤백 이미지는 최초 연결이 실물에서
  확인됐던 진단 v7 SHA `7e86f8397327b81affd521ee5638a401a3a2f68da30bb4dd7efb194bc747c225`
  기준으로 복원했다.
- 2026-08-11 13:33 KST에 재빌드하지 않은 기존 진단 v7 16MiB 이미지를 COM4에
  체크섬 고정 상태로 다시 플래시했다. 다음 확인은 reset 후 v7의 최초 연결 복구다.
- v7은 TONEX 미연결 상태에서는 화면이 안정적이었으나, TONEX 연결 직후 USB 상태
  제목이 반복해서 바뀌고 오른쪽 가장자리가 깨졌다. 이는 보드 전체의 상시 화면 결함이
  아니라 USB 상태 burst와 전체 화면 전송이 결합된 attach 회귀로 기록했다.
- 진단 v9은 v7의 USB 전원/소유권 수명주기를 유지한다. CDC IN 연속 할당만 4KiB에서
  2KiB로 줄였고 32KiB ring/stream decoder는 그대로 유지한다. 비치명 protocol frame은
  serial log에만 남기며, JC3248W535 full-frame 갱신은 50ms 간격으로 coalesce한다.
- 진단 v9은 앱 3,611,568/5,111,808바이트(70.65%), SHA-256
  `9df288fbf3fa5d29454c7e3b3f81fc5af0a68c87b2248d775d9df8dffa9e7b6e`이며
  warning-free 대상 빌드와 전체 migration verifier를 통과했다.
- 2026-08-11 15:26 KST에 위 SHA의 진단 v9 16MiB 이미지를 COM4에 체크섬 고정
  상태로 플래시했다. 다음 실물 확인은 reset 후 TONEX attach 안정성과 edge noise다.
- 진단 v9에서도 TONEX 연결이 실패했다. 2KiB RX와 화면 coalescing은 구조적 원인을
  해결하지 못했으므로 소스에서 원복하고 v9을 실패 처리했다.
- 레거시 C는 독립 USB Host client가 raw device handle을 계속 소유하면서 VID/PID 확인과
  endpoint descriptor patch를 완료한 다음 CDC를 설치·연다. 현재 Rust는 CDC를 부팅 시
  먼저 설치하고 CDC new-device callback의 임시 handle에서 descriptor를 수정한 뒤 즉시
  반환한다. 이 소유권·설치 순서 차이가 메모리 크기와 타이밍 변경에 따라 성공/실패가
  반복되는 구조적 원인이다. 다음 변경은 전용 enumerator와 raw-device 소유권을 Rust로
  복원하는 것으로 제한한다.
- Rust USB 계층에 전용 비동기 Host enumerator를 구현했다. 이 task는 raw device를 열고
  정확한 TONEX ONE VID/PID를 확인한 뒤 cached endpoint descriptor를 수정한 상태로
  handle을 유지한다. CDC driver는 이 확인 뒤에만 설치되고, 물리 분리 시 CDC close와
  uninstall이 끝난 후 enumerator가 raw handle을 닫는다.
- 최초 v10에는 과거 연결 성공만으로 raw owner 없이 CDC open을 재시도할 수 있는 경로가
  남아 있어 플래시 전에 폐기했다. v11은 전용 enumerator의 현재 `seen` 상태에서만 open한다.
- v11은 앱 3,613,072/5,111,808바이트(70.68%), SHA-256
  `8a6e03c57d48ab8c46095d2b1f257eae9f42227799b65e7e8d1763f20a5211bb`이며 전체 verifier와
  unsafe boundary 121개 감사를 통과했다. 2026-08-11 15:49 KST에 COM4로 정확한 16MiB
  이미지를 플래시했다.
- v11 실물에서도 TONEX 연결 실패, 상단 상태 반복 변경, 오른쪽 화면 깨짐이 재현됐다.
  자동 검증만 통과한 Rust 후보를 더 올리는 작업은 중단했다.
- 프로젝트에 보존된 공식 C 배포본 `V2.0.4.2_beta5_JC3248W`의 manifest와 다섯 flash
  part를 확인했다. 각 SHA-256과 주소를 고정 검증하는
  `tools/flash-legacy-jc3248w-reference.ps1`를 추가했다. 이 대조 시험은 전체 flash erase를
  포함하므로 사용자 승인 전에는 실행하지 않는다.
- 사용자 승인 후 2026-08-11 16:11 KST에 COM4 전체 flash를 지우고 공식 C 배포본의
  bootloader(0x0), partition table(0x8000), OTA state(0xd000), app(0x10000),
  skins(0x4f2000)를 고정 SHA로 기록했다. 기존 Rust 설정/NVS는 삭제됐으며 이 대조 결과에
  따라 하드웨어 문제와 Rust 회귀를 분리한다.
- 공식 C 대조 펌웨어는 같은 보드·케이블·TONEX에서 즉시 안정 연결됐다. 하드웨어,
  전원, 케이블과 TONEX 자체는 정상이며 Rust 구성만 실패한다는 결론을 고정했다.
- C와 Rust sdkconfig를 대조해 C의 `CONFIG_FREERTOS_HZ=1000`을 확인했다. 따라서 C의
  raw `vTaskDelay(500)`은 5초가 아니라 0.5초이며 Rust 상수도 500ms로 수정했다.
- Rust에 빠져 있던 `SPIRAM_TRY_ALLOCATE_WIFI_LWIP`, PSRAM instruction/rodata fetch,
  Wi-Fi IRAM 비활성화, CPU0 LwIP affinity와 BA window 6을 적용했다. 대상 앱은
  3,589,600/5,111,808바이트로 줄었다.
- v12 고정 이미지는 SHA-256
  `b6d7c879ae3a96fdb8d0efd6ddbeb64eb8932600352149de0feac2135c0b138f`이며 전체 verifier와
  16MiB locked-image plan을 통과했다. 현재 COM4가 없어 C 기준 펌웨어를 유지 중이다.
- 2026-08-11 16:28 KST에 사용자가 부트 연결한 COM4로 v12의 정확한 16MiB 이미지를
  플래시했다. 다음 실물 확인은 reset 후 약 0.5초 USB guard와 TONEX 동기화 결과다.

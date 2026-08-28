# TONEX ONE Controller

ESP32-S3 보드에서 IK Multimedia TONEX ONE만 제어하기 위한 Rust 펌웨어입니다.
이전의 혼합 장치 C 펌웨어는 제품 빌드에서 제외되며, 프로토콜·보드 배선·화면
자산을 대조하기 위한 자료로만 `legacy/`에 보존합니다.

## 주요 기능

- USB Host를 통한 TONEX ONE 자동 연결, 재연결, 20개 프리셋 동기화
- 프리셋 선택과 A/B/C 슬롯 배정
- Gate, Compressor, EQ, Amp, Cabinet/VIR, Modulation, Delay, Reverb 전체 편집
- Master Volume, Input Trim, BPM, Tempo Source, Cab/Global Bypass, Tuning Reference
- 22개 등록 보드 변형이 공유하는 반응형 보드 UI
- 프리셋 이름과 Tone Model 근거를 이용한 앰프·페달 이미지 자동 매칭
- 보드 이미지를 픽셀 손실 없이 행 단위 압축해 앱 파티션 사용량 절감
- 보드 UI와 같은 디자인 패턴의 Wi-Fi Web UI
- Wi-Fi QR과 Web 주소 QR을 분리한 빠른 연결 화면
- 보드·터치·TONEX ONE 연결 상태 화면
- LP5562 백라이트 보드의 실시간 밝기 조절과 고정 백라이트 보드의 명시적 비지원 표시
- Serial MIDI와 BLE MIDI 선택 기능
- 버전 5 설정 스키마와 버전 0~4 설정의 명시적 마이그레이션

튜너 화면과 입력원 독립적인 `no_std` YIN 음정 검출 코어, 상태별 UI와 출력
평활기는 구현되어 있습니다.
합성 기타·베이스 신호 정확도 시험도 통과했지만, TONEX ONE의 USB Audio Class
2.0 입력 adapter는 아직 검증되지 않았습니다. 따라서 production 펌웨어는 임의의
음정값을 만들지 않고 데이터 사용 불가 상태를 명확히 표시합니다. 조사 근거와
실물 검증 gate는
[튜너 구현 가능성 문서](docs/rust-migration/tuner-feasibility-2026-08-10.md)에
정리되어 있습니다.

## 프로젝트 구조

- `rust/crates/`: 도메인, 프로토콜, 애플리케이션, UI, Web, 보드 드라이버
- `rust/firmware/tonex-controller/`: ESP32-S3 제품 펌웨어
- `tools/`: 빌드·검증·성능 측정 도구
- `docs/rust-migration/`: 구조, 검증, 성능, 실물 테스트 문서
- `legacy/`: 실물 동등성 검증 전까지 보존하는 과거 구현과 비교 자료

## 소프트웨어 검증

```powershell
.\tools\verify-rust-migration.ps1
.\tools\check-all-rust-boards.ps1
.\tools\check-all-rust-boards.ps1 -WifiWeb -SerialMidi -BleMidi
```

`partitions.csv`는 최소 지원 용량인 4MiB 안에서 제품 펌웨어가 들어가도록
4,032KiB factory 앱 영역을 정의하며, `espflash.toml`이 빌드·플래시 도구에
이 배치를 일관되게 적용합니다.

2026-08-10 기준으로 다음 검증이 통과했습니다.

- 전체 Rust 테스트와 경고 금지 정적 검사
- Web UTF-8, JavaScript, 접근성, 외부 자산 비의존 검사
- 의존성 취약점·라이선스·출처·미사용 항목 검사
- UI 여섯 화면 등급의 경계 렌더와 반복 성능 측정
- JC3248W535 Wi-Fi Web 릴리스 빌드
- 22개 보드 공통 조합, BLE-only 조합, Wi-Fi Web + Serial MIDI 조합 빌드
- 8/16MiB 20개 보드의 Wi-Fi Web + Serial MIDI + BLE MIDI 최대 조합 빌드
- 4MiB 보드 2개의 금지된 Wi-Fi Web + BLE 조합 거부 검사
- YIN detector의 표준 기타 음, low bass, flat/sharp, harmonic+noise, silence,
  비주기 noise, 빠른 음 전환, meter 평활과 bounded dropout 합성 시험
- 4MiB 호환 factory 기준 Wi-Fi Web 앱 이미지 3,912,624B(94.76%) →
  3,174,192B(76.88%); JC3248W535 실제 large partition에서는 62.10%
- 압축 전후 67개 UI 미리보기 픽셀 불일치 0건

이 결과는 컴파일과 소프트웨어 동작의 증거입니다. 실제 BLE pairing, 화면 전송,
터치, USB, Wi-Fi, NVS 전원 재시작, TONEX ONE 실시간 제어와 USB audio tuner는
각 실물 검증 전까지 완료로 표시하지 않습니다.

## JC3248W535 실물 검증

소프트웨어 검증이 끝난 뒤에만 다음 문서를 따라 플래시와 실물 시험을 진행합니다.

- [실물 검증 절차](docs/rust-migration/hardware-validation.md)
- [Board/Web UI 감사](docs/rust-migration/ui-control-audit.md)
- [실물 회귀 기준표](docs/rust-migration/physical-regression-ledger.md)
- [최신 리팩터링 상태](docs/rust-migration/refactor-status-2026-07-31.md)

보드를 연결했다는 이유만으로 자동 플래시하지 않습니다. 포트와 보드 ID를 확인한
뒤 사용자가 실물 검증을 시작할 때 별도 단계로 진행합니다.

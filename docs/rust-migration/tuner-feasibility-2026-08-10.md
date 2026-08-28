# 튜너 구현 가능성 및 아키텍처 결정 - 2026-08-10

## 결론

현재 제품에서 **실제 음정값을 즉시 얻을 수 있는 검증된 입력 경로는 없다.**
따라서 production 펌웨어는 기존처럼 튜너 화면을 `unavailable` 상태로 유지하며,
가짜 note/cents 값을 표시하지 않는다.

대신 입력원과 완전히 분리된 allocation-free YIN 검출 코어를
`tonex-tuner` crate로 구현했다. 실제 TONEX ONE USB Audio Class 2.0 endpoint를
캡처하고 복합 장치에서 CDC와 isochronous audio를 동시에 안정적으로 소유하는
adapter가 검증되면 이 코어에 mono PCM frame을 전달한다.

## 확인된 하드웨어와 신호 경로

로컬 `JC3248W535 Specifications-EN.pdf`와 회로도 이미지의 근거:

- ESP32-S3-WROOM-1, 듀얼 코어 최대 240MHz
- 내부 SRAM 512KB, PSRAM 8MB, Flash 16MB
- AXS15231B 320x480 display/touch
- I2S `DIN/LRCLK/BCLK`가 NS4168 계열 speaker amplifier로 향하는 **출력** 경로
- guitar input, microphone, audio ADC/codec, input jack용 bias/protection 회로 없음
- 확장 GPIO 일부는 ESP32-S3 ADC pad로 사용할 수 있지만, 기타 pickup을 GPIO에
  직접 연결하는 것은 입력 임피던스, AC coupling, bias, clipping과 보호 문제로
  안전하지 않음

TONEX ONE 로컬 설명서와 보존된 C source의 근거:

- TONEX ONE은 USB에서 24-bit/44.1kHz audio interface로 동작
- 장치는 5-interface composite device이며 control CDC와 USB Audio Class 2.0을 포함
- 현재 Rust와 legacy controller는 CDC interface 0만 열고 음정/PCM을 읽지 않음
- legacy MIDI의 tuner CC 항목도 `Not Supported`이며, 확인된 protocol state에는
  tuner mute/thru와 A4 reference만 있고 note/cents telemetry는 없음

## 입력 아키텍처 비교

| 방식 | 정확도/지연 | CPU/RAM/대역폭 | 호환성과 위험 | 결정 |
| --- | --- | --- | --- | --- |
| A. 보드에서 pitch 검출, 결과만 Web에 전달 | PCM만 확보되면 가장 낮은 network 비용. 16kHz/2048 frame 기준 약 128ms window | 현재 YIN scratch 약 4.8KB, frame 약 8KB. Web에는 수십 byte만 전송 | TONEX UAC2 host 또는 외부 analog front-end가 필요 | **장기 목표** |
| B. 보드가 raw audio를 Web으로 전달 | 브라우저 분석은 가능하지만 추가 buffering과 Wi-Fi jitter가 생김 | 16-bit mono 16kHz만 해도 약 32KB/s + framing | 보드에 입력원이 먼저 필요하고 AP/WebSocket 부하 증가 | 제외 |
| C. 휴대폰 microphone + Web Audio | 보드 변경 없이 가능해 보이나 pedal 신호가 아닌 주변 소리를 측정 | 휴대폰 CPU 사용, 보드 부하는 낮음 | 현재 UI는 `http://tonexone.test`/`192.168.4.1`; `getUserMedia`는 secure context가 필요하고 매번 사용자 권한도 필요. 공연장 noise 영향 큼 | 제품 기본에서 제외 |
| D. 외부 high-impedance analog front-end + ADC1 DMA | 독립적으로 구현 가능하며 YIN 코어 재사용 가능 | ADC DMA와 약 13KB 고정 buffer, 낮은 network 비용 | PCB/배선 변경, 입력 보호와 anti-alias filter가 필수 | 별도 하드웨어 revision 후보 |

ESP-IDF USB Host는 복합 장치에 여러 class client와 isochronous transfer를 지원한다.
그러나 Espressif `usb_stream`의 간편 UAC host는 UAC 1.0 장치만 지원한다. TONEX
ONE은 보존된 descriptor 근거상 UAC 2.0이므로 driver를 붙이는 것만으로 검증됐다고
볼 수 없다.

공식 근거:

- ESP32-S3 ADC continuous DMA:
  https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/adc/adc_continuous.html
- ESP32-S3 USB Host composite/multiple-client/isochronous 지원:
  https://docs.espressif.com/projects/esp-usb/en/latest/esp32s3/usb_host.html
- Espressif `usb_stream` UAC 1.0 제한:
  https://docs.espressif.com/projects/esp-iot-solution/en/latest/usb/usb_host/usb_stream.html
- Web microphone secure-context 제한:
  https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia

## Pitch detection 알고리즘 비교

| 알고리즘 | 장점 | 약점 | 이 제품에서의 판단 |
| --- | --- | --- | --- |
| Zero Crossing | 매우 작고 빠름 | distortion/harmonic/noise에서 오검출, fundamental이 약하면 실패 | 기타/베이스용으로 부적합 |
| FFT peak | O(N log N), spectrum 재사용 가능 | 저음 bin 해상도와 strongest-harmonic octave 오류; interpolation/window 필요 | 단독 방식 제외 |
| Harmonic Product Spectrum | harmonic structure를 이용 | FFT와 여러 spectrum 처리, noise/inharmonicity 튜닝 필요 | 현재 자원에서 불필요하게 복잡 |
| Autocorrelation | 주기 신호에 강하고 구현 단순 | octave/subharmonic 오류와 amplitude bias | 좋은 baseline |
| McLeod/NSDF | 정확한 peak와 clarity 제공 | peak picking과 cutoff 정책이 더 복잡 | 향후 실제 capture 비교 후보 |
| YIN | normalized difference로 amplitude bias와 octave 오류를 줄임, few parameters, low latency | 제한 lag에 대해 O(N x L), 출력 smoothing이 필요 | **선택** |

YIN 원 논문은 autocorrelation 기반 방법을 수정해 오류를 줄이며 음악 신호와 낮은
지연 구현에 적합하다고 설명한다:
https://pubmed.ncbi.nlm.nih.gov/12002874/

## 구현된 검출 코어

`rust/crates/tonex-tuner`:

- `no_std`, heap allocation 없음
- 최대 4096 sample, 최대 lag 1200
- DC 제거 RMS gate
- YIN cumulative mean normalized difference
- threshold/local-minimum 선택과 fallback confidence 제한
- parabolic period interpolation
- configurable sample rate, min/max frequency, RMS, threshold, A4 reference
- frequency, target frequency, cents, MIDI note, pitch class, octave, confidence,
  RMS 결과
- weak signal과 비주기 noise는 note로 만들지 않고 typed error 반환

현재 UI 모델은 `Unavailable`, `Waiting`, `Weak`, `Stable`의 명시적 상태를 사용하며,
실제 adapter가 검증될 때까지 `Unavailable`을 유지한다. 이 코어의 존재만으로 tuner가
실제 동작한다고 표시하면 안 된다.

검출기 뒤에는 allocation-free `PitchSmoother`를 추가했다. 같은 MIDI note의 frequency와
cents는 지수 평활하지만 다른 note는 즉시 전환한다. 설정된 수의 일시적 weak/unstable
frame만 마지막 값을 유지한 뒤 `WeakSignal` 또는 `UnstablePitch`로 전환한다. 따라서 meter
떨림을 줄이면서 빠른 음 전환을 의도적으로 늦추지 않는다.

보드 Tuner UI는 note와 octave, current/target frequency, cents, flat/sharp meter,
A4 reference, mute/thru와 `입력 대기 / 약한 신호 / 안정 신호 / 입력 미지원`을 구분한다.
작은 화면에서는 핵심 note/meter를 우선하고 중형 이상에서 frequency를 추가 표시한다.

## 합성 신호 검증

16kHz, 2048-sample frame으로 다음 host test를 통과했다.

- E2 82.41Hz, A2 110.00Hz, D3 146.83Hz, G3 196.00Hz
- B3 246.94Hz, E4 329.63Hz, A4 440.00Hz
- -17 cents flat, +23 cents sharp
- fundamental + 2차 harmonic + deterministic noise
- silence/weak input 거부
- 강한 비주기 noise 거부
- B1 61.74Hz 뒤 E4로 빠른 frame 전환

정확한 음과 flat/sharp test 허용 오차는 1 cent, harmonic+noise는 2 cents이며 모두
통과했다. x86-64 release 참고 측정은 2048-sample frame당 평균 488us였다. 이는
ESP32-S3 실측값이 아니며 MCU 성능 주장으로 사용하지 않는다.

## 실제 입력을 연결하기 전 필요한 증거

1. TONEX ONE 전체 USB configuration descriptor와 alternate interface 기록
2. UAC2 AudioStreaming IN endpoint, format, channel, bit depth, sample rate 확인
3. 기존 CDC control과 audio client의 동시 claim/해제/reconnect 시험
4. isochronous 수신 중 display QSPI, Wi-Fi Web, touch, CDC parameter 변경 안정성
5. internal DMA heap, largest free block, task stack, dropped audio frame 측정
6. 실제 guitar/bass에서 octave error, noise gate, latency, smoothing 계수 측정
7. 이 증거가 통과한 뒤에만 `TunerSignalState::Waiting/Weak/Stable`과 자동 tuner page 전환 연결

외부 analog adapter를 선택한다면 high-impedance buffer, AC coupling, mid-rail bias,
input clamp/protection, anti-alias filtering과 ADC1 pin 배정을 먼저 회로 수준에서
검증해야 한다.

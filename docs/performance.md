# 메모리·CPU 측정 기록

README의 비교표는 이 장치에서 개발 중 저장한 과거 기록을 옮긴 것입니다. 동일 기능·동일 시간·동일 부하를 통제한 Zebar 대 Rust 실험은 아닙니다.

## 장치

- Microsoft Surface Pro, 11th Edition
- Windows 11 ARM64
- Snapdragon X Plus X1P-64-100, 10코어 / 논리 프로세서 10개

2026-10-19 로컬 `Win32_Processor` 조회 결과는 `Snapdragon(R) X 10-core X1P64100 @ 3.40 GHz`입니다. X Elite로 표기하지 않았습니다. 모델 분류는 [Qualcomm Snapdragon X Plus 제품 자료](https://www.qualcomm.com/content/dam/qcomm-martech/dm-assets/documents/Snapdragon-X-Plus-Product-Brief.pdf)에서도 확인할 수 있습니다.

## Zebar + WebView2

- 측정 시각: 2026-10-15 23:07 KST
- 구성: 당시 사용하던 `pretendard-bar`의 GlazeWM 연동 위젯
- React 개발용 로딩과 실행 중 Babel 변환을 배포용 번들로 바꾼 뒤 측정한 `after-production` 결과
- 재실행 후 15초 대기, 약 8.1초 CPU 측정
- Zebar와 그 자식 프로세스 전체를 부모 PID로 추적: Zebar 1개 + WebView2 12개
- 메모리는 측정 종료 시점의 프로세스별 합계: 작업 집합 799.7 MiB, Private Bytes 331.4 MiB
- 전체 CPU 용량 기준 0.058% (원본에서 소수 셋째 자리까지 저장)

자료: [원본 결과](benchmarks/zebar-after-production.json), [당시 측정 스크립트](benchmarks/measure-zebar.ps1).

## Rust GlazeBar

- 측정일: 2026-10-19, WinForms → Go → Rust 순차 실행 비교 중 Rust 구간
- 실행 후 30초 대기, 5초 간격으로 13회 수집, 실제 구간 60.2476016초
- 상주 `WorkspaceToggle.exe` 한 개를 집계하며 별도 팝업 조작을 자동 수행하지 않는 스크립트
- 작업 집합 평균 25.18359375 MiB, Private Bytes 평균 5.046274038461538 MiB
- CPU 시간 증가 0.28125초, 전체 CPU 용량 기준 0.0466823562%
- 측정 실행 파일의 SHA-256은 원본 결과에 포함

자료: [원본 결과](benchmarks/rust-runtime.json), [당시 순차 비교 스크립트](benchmarks/compare-rust-runtime.ps1).

비교 스크립트는 개발 당시 백업 바이너리와 설치 상태 파일에 의존하고, 실행 중인 표시줄을 교체합니다. 일반 설치 환경에서 바로 실행하는 용도가 아닙니다.

## 해석

- 두 기록 모두 GlazeWM 본체·watcher 및 다른 일반 앱은 제외합니다. Zebar에는 WebView2 자식 프로세스를 포함합니다.
- `CPU % = CPU 시간 증가 ÷ 실제 경과 시간 ÷ 논리 프로세서 수 × 100`입니다. 단일 코어를 100%로 잡는 표기가 아닙니다.
- 메모리 원본의 `MB`라는 필드명은 PowerShell의 `1MB`(1,048,576바이트)로 나눈 값이므로 문서에서는 MiB로 표기했습니다.
- 작업 집합 합계에는 프로세스 사이의 공유 페이지가 중복 포함될 수 있습니다. Private Bytes는 전용 커밋량이며 실제 RAM 상주량과 같지 않습니다.
- Zebar는 종료 시점의 메모리 합계, Rust는 13개 샘플의 평균입니다. 날짜·실행 구성·대기 시간·측정 시간이 다르고 당시 사용자 조작 여부도 완전히 통제되지 않았습니다.
- 이 기록은 당시 구성의 메모리 차이를 보여주는 참고 자료입니다. Zebar 전체의 일반 성능이나 CPU 성능 우위, 앱별 전력 소비, 배터리 수명 개선율로 일반화하지 않습니다.

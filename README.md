# GlazeBar

Windows용 Rust 네이티브 상태 표시줄과 GlazeWM 커뮤니티 번들입니다.
WebView2, .NET, Go 런타임을 설치하지 않고 GlazeBar를 실행할 수 있습니다.
GlazeWM의 공식 프로젝트나 공식 배포판은 아닙니다.

## 다운로드 및 설치

[Releases](https://github.com/lamininA1/GlazeBar/releases)에서 Windows 아키텍처에 맞는 ZIP을 받으세요.

- **ARM64**: Windows on ARM
- **x64**: Intel / AMD Windows

ZIP에는 GlazeWM 3.10.1 공식 MSI, GlazeBar, 설정, 라이선스와 대응 소스가 포함됩니다.
압축을 풀고 기존 GlazeWM과 GlazeBar를 종료한 다음 PowerShell에서 실행하세요.

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1
```

로그인 시 자동 시작하려면 `-AutoStart`를 추가하세요. 설치 후 시작 메뉴에서 GlazeWM을 실행합니다.
기존 파일은 먼저 백업합니다. 제거·복원은 앱을 종료한 뒤 `Uninstall.ps1`을 실행하면 됩니다.
설치 후 수정한 파일도 따로 보관합니다.

[한국어 설치 안내](distribution/README.ko.txt) · [English instructions](distribution/README.txt)

GlazeWM 공식 실행 파일에는 Microsoft Visual C++ Runtime이 필요합니다.
없으면 설치 스크립트가 Microsoft에서 직접 내려받아 서명을 확인하고 설치 화면을 표시합니다.
이 경우 인터넷과 사용자 승인이 필요합니다. GlazeBar 자체는 C 런타임을 정적으로 연결합니다.

## 주요 기능

- GlazeWM 작업공간 표시, 창 관리 단축키, 전체 화면 대응
- CPU·메모리·배터리·충방전 전력 표시
- Wi-Fi, 밝기, 디스플레이 배율, 전원 구성표 메뉴
- 배경화면 선택, 색상 추출과 테마 적용, Wallhaven 검색
- 네이티브 팝업, DPI 배율 지원

`Win+F1`: 단축키 안내 · `Win+Alt+Shift+B`: 바 표시/숨기기

## 검증 범위

현재 릴리스는 **0.1.0-preview.1**입니다.
Rust 테스트 72개, 격리된 폴더의 설치·제거·실패 복구, ZIP 체크섬 검사를 통과했습니다.
ARM64 개발 PC 및 ARM Windows의 x64 에뮬레이션에서 조회 기능을 확인했습니다.
실제 Intel/AMD PC와 새 Windows 환경의 MSI/UAC 설치 검증은 남아 있습니다.
GlazeBar 실행 파일과 스크립트에는 아직 게시자 코드 서명이 없습니다.

날씨의 기본 지역은 서울입니다. Open-Meteo 무료 API의 비상업용 서비스 조건이 적용됩니다.
장치에 따라 WMI 밝기 조절과 일부 전원 구성표를 지원하지 않을 수 있습니다.

## 소스 빌드

Windows, Rust 1.93 이상, 해당 아키텍처의 MSVC C++ 도구와 Windows SDK가 필요합니다.

```powershell
cd native-rust
rustup target add aarch64-pc-windows-msvc x86_64-pc-windows-msvc
cargo test --locked
cargo build --locked --release --target aarch64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc
```

배포 패키지는 저장소 루트에서 PowerShell 7로 생성합니다.

```powershell
.\distribution\Prepare-Upstream.ps1
.\distribution\Build-Bundle.ps1
```

실행 파일의 이름은 호환성을 위해 `WorkspaceToggle.exe`와 `GlazeTheme.exe`를 사용합니다.
같은 바이너리가 실행 파일 이름에 따라 상태 표시줄 또는 테마 도우미로 동작합니다.
실행에는 설정·아이콘도 필요하므로 처음 사용한다면 배포 ZIP의 설치 스크립트를 이용하세요.

## 라이선스와 출처

직접 작성한 GlazeBar 코드와 번들 스크립트는 **GPL-3.0-only**입니다.
GlazeWM의 GPLv3 및 각 의존성의 원래 저작권·라이선스는 유지합니다.

- [LICENSE](LICENSE)
- [제삼자 고지](THIRD-PARTY-NOTICES.txt)
- [의존성 목록과 라이선스](licenses/)
- [GlazeWM](https://github.com/glzr-io/glazewm)
- [Aether](https://github.com/omacom/aether)
- [Lucide](https://github.com/lucide-icons/lucide)

릴리스 ZIP에는 GlazeBar·GlazeWM의 대응 소스, 의존성 소스와 빌드 안내를 동봉합니다.
재배포할 때 이 자료와 라이선스 고지를 함께 유지하세요.
일부 의존성은 전문 대신 SPDX 선언만 제공하며, 해당 선언·작성자 정보와 표준 약관 및 출처를 보존했습니다.
사용자가 다운로드한 배경화면은 이 저장소나 배포 ZIP에 포함하지 않습니다.

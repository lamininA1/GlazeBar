GlazeBar + GlazeWM 커뮤니티 번들 0.1.0-preview.8

Windows 11 x64 / ARM64용입니다. PC 아키텍처에 맞는 패키지를 사용하세요.
GlazeBar와 수정 GlazeWM 3.10.1-glazebar.2가 포함됩니다.
GlazeWM 공식 배포판이 아닌 독립적인 커뮤니티 번들입니다.

최신 변경
WSL2(WSLg) 실행 시 내부 msrdc.exe를 원격 데스크탑으로 오인해 GlazeWM이
자동 일시중지되던 문제를 수정했습니다. 또한 Windows 작업 관리자(Taskmgr)와
한글(Hwp) 보조창을 기본 ignore 규칙에 추가했습니다.

설치 및 업데이트
1. ZIP 전체를 압축 해제합니다.
2. 실행 중인 GlazeWM과 GlazeBar를 종료합니다.
3. 기존 커뮤니티 번들은 먼저 제거하고 백업된 사용자 설정을 보관합니다.
4. 압축을 푼 폴더에서 실행합니다.
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1
   로그인 시 자동 시작은 -AutoStart를 추가합니다.
5. %USERPROFILE%\.glzr\glazewm\wm\glazewm.exe를 실행합니다.
통합 설치 EXE를 사용했다면 시작 메뉴의 GlazeBar + GlazeWM을 실행하세요.

설치 위치: %USERPROFILE%\.glzr\glazewm
백업 위치: %USERPROFILE%\.glzr\community-backups
기존 파일은 백업됩니다. 다른 GlazeWM 버전은 임의로 교체하지 않습니다.
GlazeWM에 필요한 Visual C++ 런타임이 없으면 Microsoft에서 내려받아
서명을 확인하고 설치 화면을 표시합니다. 이때 인터넷과 권한 확인이 필요할 수 있습니다.
GlazeBar 자체에는 .NET/Go/WebView2가 필요 없습니다.

제거 및 복원
두 프로그램을 종료하고 같은 폴더에서 실행합니다.
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Uninstall.ps1
EXE로 설치했다면 Windows 설정의 앱 목록에서 커뮤니티 번들을 제거합니다.
설치 전 파일을 복원하고 설치 후 수정한 파일은 edits-at-uninstall에 보관합니다.
기존 백업·캐시, 별도로 설치된 공식 GlazeWM, 적용한 Windows 테마·배경화면은 유지됩니다.

단축키
Win+F1: 단축키 안내
Win+Alt+Shift+B: 바 표시/숨기기
Win+Ctrl+Alt+Space: 배경화면 선택
Alt+Shift+E: GlazeWM 종료

지원 범위
ARM64 패키지는 x64에서 교차 빌드했습니다. 코드 서명은 없습니다.
밝기는 모니터별 WMI 지원 여부에 따라 활성화되며 외부 모니터 DDC/CI는 포함하지 않습니다.
날씨 기본 지역은 서울입니다. Open-Meteo 무료 API는 비상업용 조건이 적용됩니다.

라이선스 및 소스
GlazeBar와 번들 스크립트: GPL-3.0-only. GlazeWM과 의존성의 기존 라이선스는 유지합니다.
LICENSE, THIRD-PARTY-NOTICES.txt, licenses, source를 함께 배포하세요.
source에는 대응 소스와 의존성 소스, 빌드 안내가 포함됩니다.
전체 안내: https://github.com/lamininA1/GlazeBar

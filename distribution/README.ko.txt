GlazeBar + GlazeWM 커뮤니티 번들 0.1.0-preview.3

preview.3에서는 Wi-Fi를 사용할 수 없으면 해당 항목을 숨깁니다. 연결만 끊긴
어댑터는 계속 표시합니다. 팝업은 내부 클릭 없이도 외부 창 전환이나 Bar 클릭으로
닫히며, 팝업 내부 입력란과 하위 설정 창으로 이동할 때는 유지됩니다.

GlazeWM 공식 배포판이 아닌 커뮤니티 구성입니다.
Windows 11 ARM64 / x64용 ZIP을 각각 제공합니다.

설치
1. ZIP 전체를 압축 해제합니다.
2. 실행 중인 GlazeWM과 GlazeBar를 종료합니다.
3. 압축을 푼 폴더에서 PowerShell로 실행합니다.
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1
   로그인할 때 자동 시작하려면 명령 끝에 -AutoStart를 추가합니다.
4. 시작 메뉴에서 GlazeWM을 실행합니다.

기존 설정과 덮어쓰는 파일은 먼저 백업됩니다.
설치 위치: %USERPROFILE%\.glzr\glazewm
백업 위치: %USERPROFILE%\.glzr\community-backups
기존 GlazeWM이 3.10.1이 아니면 임의로 변경하지 않고 설치를 중단합니다.
GlazeWM 설치 시 Windows 관리자 권한 확인이 나타날 수 있습니다.
GlazeBar에는 .NET/Go/WebView2 설치가 필요 없습니다.
GlazeWM 공식 실행 파일에 필요한 Visual C++ 런타임이 없으면 Microsoft에서
직접 내려받아 서명을 확인한 뒤 설치 화면을 표시합니다. 이 경우 인터넷이
필요하며, Microsoft 사용 조건에 동의하고 설치를 마쳐야 합니다.

제거 및 복원
GlazeWM과 GlazeBar를 종료한 뒤 같은 폴더에서 실행합니다.
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Uninstall.ps1
설치 전 파일을 복원합니다. 설치 후 수정한 파일은 백업 폴더 안의
edits-at-uninstall에 따로 보관합니다. 기존 백업과 캐시는 남겨둡니다.
GlazeWM 자체는 남으므로 필요하면 Windows의 설치된 앱에서 제거합니다.
앱에서 직접 적용했던 Windows 테마·배경화면은 제거 시 되돌리지 않습니다.

단축키
Win+F1: 단축키 안내
Win+Alt+Shift+B: 바 숨기기/표시
Win+Ctrl+Alt+Space: 배경화면 선택
Alt+Shift+E: GlazeWM 종료

배포 상태
미리보기 버전이며 GlazeBar 및 스크립트에 코드 서명은 아직 없습니다.
preview.3의 x64 빌드는 실제 x64 Windows에서 검사했습니다. ARM64는 교차 빌드했으며
이번 수정본의 ARM64 실행 검증과 새 Windows 환경의 MSI/UAC 설치 검증은 남아 있습니다.
밝기 조절은 모니터별 WMI 지원 여부를 확인하며, 미지원이면 조작을 비활성화합니다.
외부 모니터 DDC/CI 제어는 포함하지 않습니다. 상주 프로세스 이름과 Windows 파일 설명은
GlazeBar로 변경했습니다(GlazeBar.exe).
전원 메뉴는 개인 프로필 파일이 없으면 해당 PC의 기본 전원 구성표를 사용합니다.
없는 구성표는 선택할 수 없습니다. 밝기는 장치의 WMI 지원 여부에 따라 다릅니다.
날씨는 기본적으로 서울입니다. Open-Meteo 출처와 서비스 조건은 README.txt에
있습니다. 무료 API는 비상업용 서비스이므로 상업 배포 시 별도 조정이 필요합니다.

라이선스 및 재배포
직접 작성한 GlazeBar 코드와 번들 스크립트는 GPL-3.0-only입니다.
GlazeWM의 GPLv3와 각 의존성의 기존 라이선스는 그대로 유지합니다.
LICENSE, THIRD-PARTY-NOTICES.txt, licenses 폴더, source 폴더를 함께 배포하세요.
source에는 두 프로그램의 소스, 의존성 소스와 빌드 안내가 포함되어 있습니다.
일부 원본 프로젝트는 별도 라이선스 전문 없이 SPDX 선언만 제공합니다.
그 경우 원래 선언·작성자 정보와 표준 약관 및 출처 기록을 함께 보존했습니다.
개인 배경화면, 다운로드 이미지, 사용 기록과 설정 백업은 번들에 포함하지 않습니다.

# 소스 빌드

Windows, Rust 1.93 이상, 대상 아키텍처의 MSVC C++ 도구와 Windows SDK가 필요합니다.

```powershell
cd native-rust
rustup target add aarch64-pc-windows-msvc x86_64-pc-windows-msvc
cargo test --locked
cargo build --locked --release --target aarch64-pc-windows-msvc
cargo build --locked --release --target x86_64-pc-windows-msvc
```

## 배포 번들

저장소 루트에서 PowerShell 7로 실행합니다.

```powershell
.\distribution\Prepare-Upstream.ps1
.\distribution\Build-Bundle.ps1
```

생성된 `distribution/output-날짜-시간` 경로를 다음 단계에 전달합니다.
GlazeWM 공식 MSI와 대응 소스, 의존성 소스 및 라이선스를 수집하므로 네트워크와 디스크 공간이 필요합니다.

## 설치 EXE

Inno Setup 6.7 이상을 설치한 뒤 실행합니다.

```powershell
.\distribution\Build-Setup.ps1 -BundleRoot .\distribution\output-날짜-시간
```

기본 출력은 `distribution/setup-output`입니다. Inno Setup 설치 위치가 다르면 `-Compiler`로 ISCC.exe 경로를 지정합니다.
`-TestProfile`은 별도 테스트 설치 프로그램을 만드는 개발자 옵션입니다. 실제 MSI나 사용자 설정을 변경하지 않고 격리된 프로필에서 설치·복원을 시험하며, 이 실행 파일은 배포하면 안 됩니다.

배포용 EXE의 사용자 프로필 경로 처리는 해당 아키텍처의 Windows에서 아래처럼 확인합니다. `/CHECKSTARTUP`으로 시작 검사만 수행하고 설치 전에 종료합니다.

```powershell
.\distribution\Test-SetupStartup.ps1 -SetupExe .\distribution\setup-output\GlazeBar-GlazeWM-Setup-0.1.0-preview.4-arm64.exe
```

x64 Windows에서는 파일 이름의 `arm64`를 `x64`로 바꿉니다. 이 검사는 MSI 설치·권한 확인을 포함한 전체 설치 테스트와는 별개입니다.

설치 프로그램은 현재 사용자 권한으로 실행하고, GlazeWM의 공식 MSI 및 필요한 Microsoft 런타임 설치 프로그램만 자체 권한 확인 절차를 거칩니다.
제거 과정에서 설정 복원에 실패하면 복구에 필요한 파일을 유지합니다.

상주 실행 파일은 `GlazeBar.exe`, 테마 처리 도우미는 `GlazeTheme.exe`입니다. 같은 Rust 바이너리가 이름에 따라 역할을 선택합니다. preview.1의 `WorkspaceToggle.exe`를 직접 참조하는 사용자 설정은 `GlazeBar.exe`로 변경해야 합니다. 기존 번들은 제거 후 새 번들을 설치합니다.
Windows 파일 설명과 버전은 `native-rust/glazebar.rc`에서 정의하며, 빌드에는 Windows SDK의 `rc.exe`가 PATH에 있어야 합니다.

설치 폴더의 소스로 재구성하려면 `bundle/source/glazebar-source.zip`을 풀고,
`setup-source`의 스크립트·설명 파일을 그 트리의 `distribution`에 복사하세요.
`INNO-SETUP-LICENSE.txt`는 트리 루트의 `licenses` 폴더에 두세요.
`Build-Setup.ps1`의 `-BundleRoot`에는 설치 시 포함된 `bundle` 폴더의 상위에
`GlazeBar-Community-0.1.0-preview.4-windows-arm64` 또는 `...-x64` 이름으로 배치한 경로를 지정합니다.
두 아키텍처를 모두 만들려면 양쪽 번들 입력이 필요합니다.

## preview.4 x64 modified WM

The previous Build-Bundle.ps1 recipe produces upstream-based bundles. For preview.4 use Build-ModifiedBundle.py with an existing preview.3 x64 bundle (dependency/license inputs), the GlazeBar repository, built GlazeBar executable, and modified GlazeWM checkout/runtime. Then run Build-Setup.ps1 (defaults to preview.4 x64). See docs/glazewm-modifications.md for the WM toolchain and patch. Extract glazewm-modified-source.zip into a build root, then extract glazewm-vendor.zip into its vendor/ subfolder beside Cargo.toml. Set VERSION_NUMBER=3.10.1-glazebar.1.

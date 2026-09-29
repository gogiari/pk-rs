# pk-rs (Rust Native Proxy & Tunnel Manager)

실행 시 Python이나 Node.js 의존성 없이 단일 바이너리로 동작하는 SSH 터널, HTTP-to-SOCKS5 프록시, 웹 설정 GUI 및 CLI 래퍼입니다.

## 주요 기능

1. **단일 바이너리**: Python 설치, 라이브러리 충돌 없이 단 하나의 실행 파일로 동작
2. **웹 설정 GUI (포트 8253)**: 브라우저에서 SSH 원격 대상 IP, 포트, NO_PROXY 등을 실시간 수정 및 저장
3. **고성능 HTTP-to-SOCKS5 프록시**: Tokio 기반 비동기 I/O로 SSE, WebSocket, 끊김 없는 스트리밍 지원
4. **전역 명령 설치**: `pk install` 실행 시 `codex-proxy`, `grok-proxy`, `claude-proxy` 등 자동 등록

## 사용법

### 1. 빌드 및 설치

Windows PowerShell에서 저장소 루트의 빌드 스크립트 하나로 프론트와 Rust를 순서대로 빌드합니다. Node.js와 Rust 툴체인은 빌드할 때만 필요합니다.

```powershell
.\build.ps1
```
Windows에서는 생성된 **`target\release\pk-installer.exe`를 더블클릭**하면 됩니다. 설치 파일에는 `pk.exe`가 포함되어 있으며 관리자 권한 없이 `%LOCALAPPDATA%\pk\bin`에 설치합니다. 시작 메뉴와 바탕화면 바로가기를 만들고 사용자 PATH에 명령을 등록합니다. 설치가 끝나면 대시보드를 바로 열지 선택할 수 있습니다. **새 PowerShell 창**에서는 `pk`, `pk status`, `codex-proxy` 등을 실행할 수 있습니다. 설치 파일은 빌드 후 단독으로 전달할 수 있습니다. 제거할 때는 Windows의 **설치된 앱**에서 `PK Proxy Manager`를 선택하거나 시작 메뉴의 **Uninstall PK Proxy Manager**를 실행합니다. 제거 시 프로그램과 명령어·바로가기는 삭제하고 개인 설정은 보존합니다.

Linux x86_64용 설치 파일은 Linux 환경에서 `bash build-linux.sh`로 만듭니다. Node.js 20.19 이상 또는 22.12 이상, Rust/rustup, C 컴파일러(`cc`), [nFPM](https://nfpm.goreleaser.com/docs/install/)이 필요합니다. 스크립트는 정적 실행 파일을 빌드하고 결과물을 `target/linux-dist`에 생성합니다. `bash verify-linux.sh`로 패키지와 실행 파일을 검증할 수 있습니다.

| 배포판 | 설치 파일 | 사용 방법 |
| --- | --- | --- |
| Ubuntu·Debian 계열 | `.deb` | 파일을 패키지 설치기로 열거나 `sudo apt install ./pk-proxy-manager_*.deb` |
| Fedora·RHEL 계열 | `.rpm` | 파일을 패키지 설치기로 열거나 `sudo dnf install ./pk-proxy-manager-*.rpm` |
| Alpine | `.apk` | `sudo apk add --allow-untrusted ./pk-proxy-manager_*.apk` |
| Arch 계열 | `.pkg.tar.zst` | `sudo pacman -U ./pk-proxy-manager-*.pkg.tar.zst` |
| 그 외 x86_64 Linux | `.tar.gz` | 압축을 풀고 `./install.sh` 실행; 사용자 폴더에 설치 |

배포판 패키지는 `pk` 명령과 앱 메뉴 항목을 함께 설치하고, 패키지 관리자로 제거할 수 있습니다. 범용 묶음은 `./uninstall.sh`로 제거합니다. 터널을 사용하려면 OpenSSH의 `ssh` 명령이 필요합니다. Windows에서도 `pk install`을 직접 실행할 수 있지만 일반적인 설치에는 필요하지 않습니다.

macOS에서는 Mac에서 `bash build-macos.sh`를 실행하면 `target/macos-dist/pk-proxy-manager_<버전>_macos-universal.pkg`가 만들어집니다. Node.js 20.19 이상 또는 22.12 이상, Rust/rustup, Xcode Command Line Tools가 필요합니다. 패키지는 Intel·Apple Silicon 공용 앱을 `/Applications/PK Proxy Manager.app`에 설치하고 `/usr/local/bin`에 `pk`와 프록시 명령을 등록합니다. Finder에서 앱을 열거나 새 터미널에서 `pk ui`를 실행하면 됩니다. 제거할 때는 `/Applications/Uninstall PK Proxy Manager.command`를 실행합니다. 실행 중이라면 `pk stop`으로 종료한 뒤 제거하세요. 개인 설정은 `~/.config/pk`에 남습니다. 기본 빌드는 서명되지 않습니다. 외부 배포용으로 서명·공증하려면 Mac의 Developer ID 인증서를 준비하고 `PK_MACOS_APP_SIGN_IDENTITY`, `PK_MACOS_INSTALLER_SIGN_IDENTITY`, `PK_MACOS_NOTARY_PROFILE` 환경변수를 설정한 뒤 빌드합니다.
스크립트는 처음 실행하거나 `web/package.json` 또는 `web/package-lock.json`이 바뀌었을 때만 `npm ci`를 실행하고, UI 소스가 바뀌었을 때만 React를 다시 빌드합니다. React 화면은 `web/dist`에 빌드되고 Rust 릴리스 바이너리에 포함됩니다. Windows 배포에는 `pk-installer.exe` 파일 하나를 전달하면 됩니다.

### 2. 웹 대시보드 및 백그라운드 프록시 실행
Windows에서는 빌드된 `target\release\pk.exe`를 더블클릭하면 백그라운드 서비스가 시작되고 브라우저에서 대시보드가 열립니다. 명령 없이 터미널에서 실행해도 같습니다.

```bash
pk ui
# 또는
pk start
```
- 브라우저에서 `http://127.0.0.1:8253` 접속하여 SSH 대상(예: `user@example.com`) 설정 및 상태 확인
- 웹 대시보드와 HTTP 프록시는 IPv4 루프백 주소 `127.0.0.1`에만 바인딩되어 같은 컴퓨터에서만 직접 접속할 수 있습니다. 화면 오른쪽 위에서 시스템/라이트/다크 테마를 선택할 수 있습니다. 기본값인 시스템은 운영체제 테마 변경을 따르며, 선택은 브라우저에 저장됩니다. 대시보드는 초광폭 화면에서 최대 폭을 제한하고 중간·모바일 화면에서는 열을 재배치합니다.

### 웹 UI 수정

- `web/src/App.tsx`: React 화면과 상태 관리
- `web/src/styles.css`: 디자인과 레이아웃
- `web/src/api.ts`: Rust의 `/api/*` 호출

UI를 수정한 뒤 `.\build.ps1`을 다시 실행하면 배포용 실행 파일에 반영됩니다. 개발 중에는 두 터미널에서 각각 다음 명령을 실행하세요.

```powershell
# 터미널 1: Rust 백엔드 (저장소 루트)
cargo run -- start --foreground

# 터미널 2: React 프론트 (저장소 루트)
npm --prefix web run dev
```

브라우저에서는 `http://127.0.0.1:5173/`을 여세요. `web` 폴더 안에서는 `npm run dev`를 실행해도 됩니다. Vite는 기본 웹 포트 `8253`의 `/api` 요청을 Rust로 전달합니다. 새로 받은 저장소에서 Rust 개발 서버가 `web/dist`가 없다는 오류로 빌드되지 않으면 `.\build.ps1`을 한 번 실행하세요.

### 3. 바로가기 명령어 사용
설치 후 터미널 어디서든 바로 호출:
```bash
codex-proxy
grok-proxy
claude-proxy
agy-proxy
ocx-proxy
```

# pk-rs (Rust Native Proxy & Tunnel Manager)

Rust로 만든 SSH 터널, HTTP-to-SOCKS5 프록시, 웹 설정 GUI 및 CLI 래퍼입니다. npm으로 설치해도 실제 서비스는 네이티브 실행 파일로 동작합니다.

## 빠른 설치 (npm)

Node.js 18 이상이 설치되어 있다면 Windows x64, macOS Intel/Apple Silicon, Linux x64에서 같은 방식으로 설치할 수 있습니다.

```bash
npm install -g @gomul82/pk
pk ui
```

패키지 정보와 배포 버전은 [npm](https://www.npmjs.com/package/@gomul82/pk)에서 확인할 수 있습니다.

`pk`, `codex-app-proxy`, `chatgpt-pk`, `codex-proxy`, `grok-proxy`, `claude-proxy`, `agy-proxy`, `ocx-proxy`, `opencodex-proxy`가 전역 명령으로 등록됩니다. npm 설치에서는 `pk install`을 따로 실행하지 마세요. 업데이트 전에는 `pk stop`으로 서비스를 종료한 뒤 `npm install -g @gomul82/pk@latest`를 실행하세요. 제거할 때도 `pk stop` 후 `npm uninstall -g @gomul82/pk`를 사용합니다. 프록시를 사용하려면 시스템에 OpenSSH `ssh` 명령이 있어야 합니다. npm이 설치를 담당하므로 프로그램 실행과 데이터 파일은 별개이며, 제거 시 개인 설정은 유지됩니다.

기존 `pk-proxy-manager` 패키지에서 이동하려면 `pk stop` 후 `npm uninstall -g pk-proxy-manager`를 실행하고 `npm install -g @gomul82/pk`로 설치하세요. 두 패키지는 같은 전역 명령을 등록하므로 기존 패키지를 먼저 제거합니다. 개인 설정은 유지됩니다.

배포는 npm으로 제공합니다. OS별 설치 파일과 앱 메뉴·바탕화면 바로가기는 제공하지 않습니다.

## 주요 기능

1. **단일 바이너리**: Python 설치, 라이브러리 충돌 없이 단 하나의 실행 파일로 동작
2. **웹 설정 GUI (포트 8253)**: 브라우저에서 SSH 원격 대상 IP, 포트, NO_PROXY 등을 실시간 수정 및 저장
3. **고성능 HTTP-to-SOCKS5 프록시**: Tokio 기반 비동기 I/O로 SSE, WebSocket, 끊김 없는 스트리밍 지원
4. **전역 명령 설치**: `npm install -g @gomul82/pk` 실행 시 `codex-proxy`, `grok-proxy`, `claude-proxy` 등 자동 등록

## 사용법

### 1. 소스에서 빌드

일반 사용자는 위의 npm 설치 명령을 사용하세요. 소스에서 빌드하려면 Node.js 20.19 이상 또는 22.12 이상과 Rust 툴체인이 필요합니다.

Windows PowerShell에서는 저장소 루트의 빌드 스크립트로 프론트와 Rust를 순서대로 빌드합니다.

```powershell
.\build.ps1
```

생성된 실행 파일은 `target/release/pk.exe`이며, npm용 실행 파일은 `npm/binaries/win32-x64/pk.exe`에 복사됩니다. 스크립트는 처음 실행하거나 의존성이 바뀌었을 때 `npm ci`를 실행하고, UI 소스가 바뀌었을 때 React를 다시 빌드합니다. React 화면은 `web/dist`에 빌드되고 Rust 실행 파일에 포함됩니다.

Linux x86_64에서는 `bash build-linux.sh`를 실행합니다. Rust/rustup, C 컴파일러와 musl 빌드 도구가 필요하며, npm용 정적 실행 파일은 `npm/binaries/linux-x64/pk`에 생성됩니다.

macOS에서는 `bash build-macos.sh`를 실행합니다. Rust/rustup과 Xcode Command Line Tools가 필요하며, Intel·Apple Silicon 공용 실행 파일은 `npm/binaries/darwin-universal/pk`에 생성됩니다.

Linux·macOS 빌드는 Actions에서 npm 패키지를 조립할 실행 파일도 `target/npm-assets`에 저장합니다. 로컬 OS의 실행 파일만으로는 모든 OS를 지원하는 npm 패키지를 만들 수 없습니다. 배포용 패키지 조립과 검증은 GitHub Actions에서 수행합니다.

### 2. 웹 대시보드 및 백그라운드 프록시 실행
Windows에서는 빌드된 `target\release\pk.exe`를 더블클릭하면 백그라운드 서비스가 시작되고 브라우저에서 대시보드가 열립니다. 명령 없이 터미널에서 실행해도 같습니다.

```bash
pk ui
# 또는
pk start
```
- 브라우저에서 `http://127.0.0.1:8253` 접속하여 SSH 대상(예: `user@example.com`) 설정 및 상태 확인
- 새 설치에서는 SSH 대상이 비어 있고 자동 연결이 꺼져 있습니다. 각자 대상과 인증 정보를 설정한 뒤 연결하세요.
- 웹 대시보드와 HTTP 프록시는 IPv4 루프백 주소 `127.0.0.1`에만 바인딩되어 같은 컴퓨터에서만 직접 접속할 수 있습니다. 화면 오른쪽 위의 해·달 버튼을 클릭하면 라이트·다크 테마가 전환되고 선택은 브라우저에 저장됩니다. 선택하기 전에는 운영체제 테마를 따릅니다. 대시보드는 초광폭 화면에서 최대 폭을 제한하고 중간·모바일 화면에서는 열을 재배치합니다.

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

### 4. ChatGPT / Codex 데스크톱 실행

대시보드의 **ChatGPT / Codex 데스크톱**에서 실행 환경을 선택하고 저장합니다. Windows에서는 Windows 앱 또는 WSL의 Linux 앱을 선택하고 WSL 배포판도 지정할 수 있습니다. Linux·macOS에서는 해당 OS에 설치된 앱을 실행합니다. 현재 컴퓨터에서 실행할 수 없는 OS는 비활성화됩니다. 앱은 별도로 설치해야 하며 WSL GUI 실행에는 WSLg가 필요합니다.

```bash
codex-app-proxy                      # 저장한 환경으로 데스크톱 실행
codex-app-proxy wsl --distro Ubuntu  # 이번 실행만 WSL / Ubuntu
chatgpt-pk                           # 저장한 환경, 기본값은 현재 OS
chatgpt-pk windows                   # 이번 실행만 Windows
chatgpt-pk wsl --distro Ubuntu       # Windows에서 WSL 앱 실행
chatgpt-pk linux                     # Linux에서 실행
chatgpt-pk macos                     # macOS에서 실행
pk desktop set wsl --distro Ubuntu   # 기본 실행 환경 저장
pk desktop list
pk desktop reset
```

`codex-app-proxy`와 `chatgpt-pk`는 같은 명령입니다. `codex-proxy`는 Codex CLI 실행용입니다.

소스 빌드에서는 `./target/release/pk.exe desktop wsl --distro Ubuntu`처럼 실행할 수 있습니다. Windows에서 WSL 앱을 실행하려면 npm 패키지에 포함된 Linux PK 바이너리가 필요합니다. 로컬 개발 중에는 `bash build-linux.sh`로 `npm/binaries/linux-x64/pk`도 빌드하세요. 앱을 찾지 못하면 웹의 **앱 실행 위치 설정** 또는 `--path`로 실행 파일을 지정합니다. WSL에는 Linux 경로, macOS에는 `.app/Contents/MacOS` 안의 실행 파일을 지정합니다.

PK가 프록시 환경 변수와 Electron 실행 옵션을 앱과 자식 프로세스에만 적용합니다. Windows·WSL·Linux·macOS 전역 프록시 설정은 변경하지 않습니다. Windows에서 WSL로 실행할 때는 루프백 중계로 Windows의 PK 프록시에 연결하며, LAN 공개나 WSL 전역 프록시 설정이 필요하지 않습니다. 이 앱 경로의 OpenAI HTTPS 요청은 로컬 DNS로 주소를 확인한 뒤 PK에 전달하고 TLS 검증은 앱에서 수행합니다.

앱은 PK 전용 프로필과 Codex 홈으로 실행하므로 처음에는 다시 로그인해야 합니다. 로그인과 대화는 이후 유지됩니다. 프록시 포트 등 설정을 바꾸었다면 PK로 실행한 앱을 닫고 다시 실행하세요. 로그는 PK 설정 폴더의 `desktop/desktop.log`에 저장합니다. WSL 앱 프로필은 해당 배포판의 `~/.local/share/pk/desktop/wsl`에 저장합니다.

### 5. 프록시 전용 브라우저 실행

로컬 빌드로 먼저 확인하려면 PowerShell에서 `./scripts/preview-browsers.ps1`을 실행하세요. `target/browser-preview`에 별도 설정을 만들고 웹 포트 `18253`, HTTP 포트 `13128`, SOCKS5 포트 `11080`을 사용합니다. 기존 설치본과 분리해 확인할 수 있습니다. `-Foreground`를 붙이면 터미널에서 실행하며 Ctrl+C로 종료합니다. 백그라운드 미리보기를 중지하려면 `$env:PK_CONFIG_DIR = "$PWD/target/browser-preview"` 설정 후 `./target/release/pk.exe stop`을 실행합니다.

대시보드의 **프록시 브라우저**에서 Chrome, Edge, Firefox를 실행할 수 있습니다. 먼저 SSH 터널을 연결하세요. 실행 전 SOCKS5 응답을 확인하며, 설정된 SOCKS 포트를 사용합니다. 브라우저별 PK 전용 프로필에 로그인과 쿠키가 유지됩니다. 실행 위치나 프록시 설정을 변경했다면 기존 PK 브라우저 창을 모두 닫고 다시 실행하세요.

`pk browser`는 OS 기본 브라우저를 PK 프록시 설정으로 실행합니다. 대시보드의 **기본 프록시 브라우저**를 선택하고 저장하면 웹의 **기본 브라우저 실행**과 CLI에 함께 적용됩니다. **OS 기본 브라우저**로 저장하면 다시 OS 설정을 따릅니다. 선택은 PK 설정에만 저장하며 OS 기본값은 변경하지 않습니다. `pk browser chrome`처럼 이름을 지정하면 저장된 기본 선택보다 우선합니다. OS 기본 브라우저가 지원 목록에 없거나 조회되지 않으면 웹에서 지원하는 브라우저를 선택하라는 오류를 표시합니다.

로컬 미리보기 웹에서 저장한 선택을 CLI로 확인할 때는 설치된 `pk` 대신 로컬 빌드와 같은 미리보기 설정 폴더를 사용하세요. 프로젝트 폴더의 별도 PowerShell 터미널에서 다음과 같이 실행합니다.

```powershell
$env:PK_CONFIG_DIR = "$PWD/target/browser-preview"
./target/release/pk.exe browser
```

Windows는 사용자별 HTTP/HTTPS 연결 앱과 실제 실행 파일 경로를 조회하고, macOS는 Launch Services의 기본 앱과 위치를 조회합니다. Linux는 `xdg-mime`, `xdg-settings`로 표준 Chrome/Chromium·Edge·Firefox desktop ID를 조회하며 Flatpak·Snap도 구분합니다. 별도 이름의 사용자 정의 desktop 항목은 PK 웹에서 브라우저와 실행 위치를 지정하세요.

**WSL**에서는 Windows 기본 브라우저와 Windows에 설치된 Chrome·Edge·Firefox를 자동으로 조회합니다. Windows에 PK를 따로 설치하지 않아도 WSL에서 Windows 실행 파일을 직접 실행합니다. 웹에서 저장한 기본 선택과 `pk browser <브라우저>`도 동일하게 적용됩니다. 수동 실행 위치에는 `C:\Apps\Browser\browser.exe` 같은 Windows 절대 경로나 `/mnt/c/Apps/Browser/browser.exe` 같은 WSL 경로를 사용할 수 있습니다.

WSL에서 Windows 브라우저를 열기 전에는 WSL과 Windows 양쪽에서 `127.0.0.1:<PK SOCKS 포트>`의 SOCKS5 응답을 확인합니다. WSL의 Windows interop, localhost 전달이 꺼져 있거나 Windows의 다른 프로그램이 같은 포트를 사용하면 설정 확인이 필요합니다. Windows 브라우저 프로필은 Windows 드라이브에 저장하고 경로를 변환합니다. PK 설정이 Windows 드라이브에 있으면 설정 폴더의 `browser-profiles`를 사용하고, Linux 파일시스템에 있으면 Windows `%LOCALAPPDATA%\pk\wsl-browser-profiles` 아래에 배포판·설정별로 저장합니다. 명시적으로 저장한 Linux 브라우저 실행 위치도 계속 사용할 수 있습니다.

WSL 연결을 개발 중 검증하려면 WSL 안에서 Linux 실행 파일을 빌드한 뒤 Windows에서 `python scripts/smoke-wsl-browsers.py <WSL의 pk 실행 파일 경로> Ubuntu`를 실행합니다. Windows의 `rustc`로 테스트용 `.exe`를 만들고 실제 WSL 서버와 Windows interop을 통해 기본 브라우저 조회, 한글 실행 위치, 프로필 변환, Windows 쪽 SOCKS5 연결과 오류 차단을 검사합니다. 실제 브라우저를 실행하는 대신 인자를 기록하는 테스트 실행 파일을 사용합니다.

```bash
pk browser                            # PK 웹 선택 또는 OS 기본 브라우저
pk browser https://example.com        # 기본 브라우저로 URL 열기
pk browser list
pk browser edge
pk browser chrome https://example.com
pk browser firefox
pk browser safari https://example.com   # macOS
```

**Safari (macOS)**는 macOS 네트워크 프록시 설정을 사용하며 기존 Safari 프로필로 실행합니다. 대시보드에서 현재 설정과 설정 방법을 확인하거나 `pk browser safari --setup`으로 안내를 확인하세요. 시스템 설정의 사용 중인 네트워크 연결에서 HTTP·HTTPS를 모두 `127.0.0.1:<PK HTTP 포트>`로 지정하거나 SOCKS를 `127.0.0.1:<PK SOCKS 포트>`로 지정한 뒤 **다시 검색**을 누릅니다. PAC·자동 검색, 다른 프록시, 확인할 수 없는 기본·인터페이스별 설정이 있으면 실행을 막고 이유를 안내합니다. 실행 직전 시스템 프록시를 다시 확인하고 SOCKS5 응답을 검사합니다. HTTP·HTTPS 방식이면 로컬 HTTP 포트도 확인합니다.

Safari의 시스템 프록시는 해당 설정을 따르는 다른 앱에도 영향을 줍니다. PK가 시스템 설정을 자동으로 변경하거나 복원하지 않으므로, PK 프록시를 종료한 뒤에는 직접 원래 설정으로 되돌리세요. Safari는 macOS의 프록시 제외 목록을 사용하며 PK의 `NO_PROXY`를 자동으로 복사하지 않습니다. [Apple의 Safari 프록시 설정 안내](https://support.apple.com/ko-kr/guide/safari/ibrw1053/mac)를 참고하세요. Windows·Linux에서는 Safari 실행 항목이 비활성화됩니다.

Windows는 레지스트리 App Paths와 사용자·시스템 설치 폴더, macOS는 `/Applications`와 `~/Applications`, Linux는 PATH와 Flatpak·Snap에서 검색합니다. Chrome 항목은 Linux의 Chromium도 검색합니다. 기본 경로 밖에 설치했다면 대시보드의 **실행 위치 설정**에서 절대 경로 또는 PATH에 있는 명령 이름을 저장하세요. 실행 파일 경로와 실행 옵션은 분리하며 임의의 셸 명령은 입력하지 않습니다. macOS의 수동 경로는 `.app` 폴더 내부 `Contents/MacOS` 실행 파일을 지정합니다.

```powershell
pk browser set edge "D:\Apps\Edge\msedge.exe"
pk browser reset edge
```

Linux 샌드박스 패키지는 실행 방식과 앱 이름을 따로 지정할 수 있습니다.

```bash
pk browser set firefox --flatpak org.mozilla.firefox
pk browser set firefox --snap firefox
```

일반 설치의 전용 프로필은 PK 설정 폴더 아래 `browser-profiles/<브라우저>`에 저장됩니다. Flatpak·Snap은 해당 앱의 사용자 데이터 폴더에 저장합니다. 실행 위치 설정은 CLI와 대시보드가 공유하며, CLI에서 변경한 뒤에는 대시보드의 **다시 검색**을 누르세요.

프록시 제외 대상에는 루프백과 현재 `NO_PROXY` 설정을 적용합니다. 도메인, `*.도메인`, IP, CIDR을 사용할 수 있으며, 브라우저에서 지원하지 않는 형태는 실행 시 오류로 안내합니다. 전체 제외를 뜻하는 `*`는 지원하지 않습니다. 원격 DNS 설정은 일반 웹 요청을 위한 것으로, 브라우저의 모든 통신을 강제로 터널링하는 기능은 아닙니다.

Chrome·Edge의 SOCKS5 웹 요청은 브라우저 자체 동작으로 대상 호스트의 DNS를 프록시에서 처리합니다. 경고 배너를 유발하는 `--host-resolver-rules`와 경고를 숨기기 위한 `--test-type`은 사용하지 않습니다. 프록시 제외 대상은 기존처럼 직접 연결하며, 브라우저의 모든 통신을 터널링하는 설정은 아닙니다. 이전 옵션으로 열린 PK 브라우저 창은 모두 닫고 다시 실행해야 새 실행 옵션이 적용됩니다.

## 설치 및 업데이트

대시보드는 열 때와 30분마다 npm에서 새 버전을 확인합니다. npm 전역 설치의 새 버전이 있으면 **npm으로 업데이트**를 누르고 연결 중단 안내를 확인한 뒤 **업데이트 시작**을 누르세요. 별도 Node.js 프로세스가 PK 종료, 같은 npm 전역 설치 위치에 새 버전 설치, PK 재시작을 처리합니다. 완료되면 화면을 자동으로 새로고침합니다. 업데이트 중 프록시 연결이 잠시 끊기며, 저장된 자동 연결 설정이 없다면 다시 연결해야 합니다. 개인 설정은 유지됩니다.

UI 업데이트는 0.1.8부터 지원합니다. 이전 버전이나 npm 외 설치는 `pk stop` 후 `npm install -g @gomul82/pk@latest`, `pk ui`로 한 번 업데이트하세요. PK를 npm 명령으로 실행해야 설치 환경을 확인할 수 있습니다. WSL의 Linux PK는 Linux npm으로 업데이트하며 Windows PK 설치는 필요하지 않습니다. 쓰기 권한이나 npm 설치 환경이 맞지 않으면 실행 전에 이유를 안내합니다. npm 설치가 실패하면 기존 실행 파일의 복사본으로 PK를 다시 시작하여 오류를 확인할 수 있게 합니다. 설치 로그는 설정 폴더의 `npm-update/npm.log`에 저장합니다. 필요하면 터미널에서 npm으로 재설치한 뒤 `pk restart`를 실행하세요.

제거할 때는 `pk stop` 후 `npm uninstall -g @gomul82/pk`를 실행합니다. 개인 설정은 유지됩니다.

## 자동 빌드 및 npm 배포

GitHub의 `main` 푸시, Pull Request, 수동 실행에서 Windows·Linux·macOS 실행 파일을 빌드하고 Rust 코드, 대시보드와 npm 실행을 검증합니다. 세 OS 실행 파일을 하나의 npm 패키지로 묶고 포함 파일과 실행 권한을 검사합니다. 검증한 npm 패키지(`.tgz`)는 해당 Actions 실행의 Artifacts에 저장됩니다. OS별 설치 파일과 GitHub Release는 생성하지 않습니다.

새 버전을 배포할 때는 `Cargo.toml`, `web/package.json`, `web/package-lock.json`, `npm/package.json`의 버전을 함께 바꾸고 `main`에 푸시한 다음, 동일한 버전의 태그를 푸시합니다. 예를 들어 버전이 `0.2.0`이라면 `v0.2.0` 태그를 푸시합니다. 태그와 각 파일의 버전이 일치하고 세 OS 빌드와 npm 패키지 검증이 모두 성공해야 npm 배포 단계가 실행됩니다. `main` 푸시, PR과 일반 수동 실행에서는 npm에 게시하지 않습니다.

npm의 `@gomul82/pk` 패키지에 `build.yml`을 신뢰 게시자로 등록하고 저장소 변수 `NPM_TRUSTED_PUBLISHER_ENABLED=true`를 설정하면 태그 빌드에서 자동 게시됩니다. 별도 npm 토큰은 필요하지 않습니다. 이미 게시한 버전은 덮어쓰지 않고 새 버전 태그를 만듭니다.

# PK Proxy Manager

A native Rust SSH tunnel and HTTP proxy manager with a local web dashboard.

```sh
npm install -g @gomul82/pk
pk ui
```

The package includes prebuilt Windows x64, macOS Intel/Apple Silicon, and Linux x64 binaries. No Rust toolchain is required. OpenSSH (`ssh`) is required to use SSH tunnels.

Commands: `pk`, `codex-app-proxy`, `chatgpt-pk`, `codex-proxy`, `grok-proxy`, `claude-proxy`, `agy-proxy`, `ocx-proxy`, `opencodex-proxy`.

Use **ChatGPT / Codex desktop** in the dashboard to save an execution environment. Windows supports its native app and WSL Linux apps; Linux and macOS run their native apps. Unsupported environments are disabled. Install the desktop app separately; WSL GUI apps require WSLg.

```sh
codex-app-proxy                    # saved desktop environment
codex-app-proxy wsl --distro Ubuntu
chatgpt-pk                         # saved environment, current OS by default
chatgpt-pk windows                 # one-time override on Windows
chatgpt-pk wsl --distro Ubuntu      # WSL app from Windows
chatgpt-pk linux                   # native Linux
chatgpt-pk macos                   # native macOS
pk desktop set wsl --distro Ubuntu
pk desktop list
```

`codex-app-proxy` and `chatgpt-pk` are aliases for the same desktop launcher. `codex-proxy` runs the Codex CLI.

Use `--path` for a custom executable. Proxy variables and Electron options apply only to the app and its children. WSL uses a loopback stdio relay to reach the Windows PK proxy, without changing system proxy settings or exposing a LAN port. OpenAI HTTPS requests on this app route use locally resolved addresses while the app retains TLS verification. PK uses a separate persistent app profile and Codex home, so sign in again on first launch. Close the PK app before changing proxy ports. Logs are in the PK configuration directory under `desktop/desktop.log`.

Connect the SSH tunnel in the dashboard, then run `pk browser` to open the OS default browser with PK's proxy settings. Save a **default proxy browser** in the dashboard to use another browser for both the web launch button and CLI. An explicit command such as `pk browser firefox https://example.com` takes priority over that preference.

Chrome, Edge, and Firefox use persistent PK profiles. Use `pk browser list` to check detected browsers, or `pk browser set edge "D:\Apps\Edge\msedge.exe"` to save a custom executable. Linux Flatpak and Snap launchers can also be configured in the dashboard. Unknown OS default browsers require selecting a supported browser in PK.

In WSL, PK detects and launches Windows browsers directly, including the Windows default browser. Windows does not need a separate PK installation. Custom paths may use Windows drive paths or WSL mount paths. Windows browser profiles stay on a Windows drive, and PK checks the SOCKS proxy from both WSL and Windows before launching. Windows interop and localhost forwarding must be available.

Safari is supported on macOS using the existing Safari profile and macOS network proxy settings. Run `pk browser safari --setup` for setup instructions. These system settings also affect other apps; restore them yourself when stopping the proxy. PK checks the settings before launching Safari and does not change them automatically.

Stop the service with `pk stop` before updating or removing the package. Then run either `npm install -g @gomul82/pk@latest` to update or `npm uninstall -g @gomul82/pk` to remove it.

From v0.1.8, the dashboard checks npm at startup and every 30 minutes. Use its npm update button to stop PK, install into the same global prefix, and restart. Proxy connections briefly disconnect; reconnect if automatic connection is not configured. Settings are preserved. Installation failures restart the previous executable so the dashboard can report the error. If npm or write permissions are unavailable, use the terminal commands above. Older versions need one terminal update to enable this button.

Do not run `pk install`; npm registers the commands already. Distribution is supported through npm only. Native installers and desktop shortcuts are not provided.

The dashboard listens on `127.0.0.1` only. Configuration is saved outside the npm package and is retained on removal.

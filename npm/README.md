# PK Proxy Manager

A native Rust SSH tunnel and HTTP proxy manager with a local web dashboard.

```sh
npm install -g @gomul82/pk
pk ui
```

The package includes prebuilt Windows x64, macOS Intel/Apple Silicon, and Linux x64 binaries. No Rust toolchain is required. OpenSSH (`ssh`) is required to use SSH tunnels.

Commands: `pk`, `codex-proxy`, `grok-proxy`, `claude-proxy`, `agy-proxy`, `ocx-proxy`, `opencodex-proxy`.

Connect the SSH tunnel in the dashboard, then run `pk browser` to open the OS default browser with PK's proxy settings. Save a **default proxy browser** in the dashboard to use another browser for both the web launch button and CLI. An explicit command such as `pk browser firefox https://example.com` takes priority over that preference.

Chrome, Edge, and Firefox use persistent PK profiles. Use `pk browser list` to check detected browsers, or `pk browser set edge "D:\Apps\Edge\msedge.exe"` to save a custom executable. Linux Flatpak and Snap launchers can also be configured in the dashboard. Unknown OS default browsers require selecting a supported browser in PK.

Safari is supported on macOS using the existing Safari profile and macOS network proxy settings. Run `pk browser safari --setup` for setup instructions. These system settings also affect other apps; restore them yourself when stopping the proxy. PK checks the settings before launching Safari and does not change them automatically.

Stop the service with `pk stop` before updating or removing the package. Then run either `npm install -g @gomul82/pk@latest` to update or `npm uninstall -g @gomul82/pk` to remove it.

Do not run `pk install` for the npm package; npm registers the commands already. For a desktop launcher or a Node-free installation, use the [native installers](https://github.com/gogiari/pk-rs/releases/latest).

The dashboard listens on `127.0.0.1` only. Configuration is saved outside the npm package and is retained on removal.

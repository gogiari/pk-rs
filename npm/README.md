# PK Proxy Manager

A native Rust SSH tunnel and HTTP proxy manager with a local web dashboard.

```sh
npm install -g @gomul82/pk
pk ui
```

The package includes prebuilt Windows x64, macOS Intel/Apple Silicon, and Linux x64 binaries. No Rust toolchain is required. OpenSSH (`ssh`) is required to use SSH tunnels.

Commands: `pk`, `codex-proxy`, `grok-proxy`, `claude-proxy`, `agy-proxy`, `ocx-proxy`, `opencodex-proxy`.

Stop the service with `pk stop` before updating or removing the package. Then run either `npm install -g @gomul82/pk@latest` to update or `npm uninstall -g @gomul82/pk` to remove it.

Do not run `pk install` for the npm package; npm registers the commands already. For a desktop launcher or a Node-free installation, use the [native installers](https://github.com/gogiari/pk-rs/releases/latest).

The dashboard listens on `127.0.0.1` only. Configuration is saved outside the npm package and is retained on removal.

# PK Proxy Manager

A native Rust SSH tunnel and HTTP proxy manager with a local web dashboard.

```sh
npm install -g pk-proxy-manager
pk ui
```

The package includes prebuilt Windows x64, macOS Intel/Apple Silicon, and Linux x64 binaries. No Rust toolchain is required. OpenSSH (`ssh`) is required to use SSH tunnels.

Commands: `pk`, `codex-proxy`, `grok-proxy`, `claude-proxy`, `agy-proxy`, `ocx-proxy`, `opencodex-proxy`.

```sh
npm install -g pk-proxy-manager@latest  # update
npm uninstall -g pk-proxy-manager       # remove
```

Do not run `pk install` for the npm package; npm registers the commands already. For a desktop launcher or a Node-free installation, use the [native installers](https://github.com/gogiari/pk-rs/releases/latest).

The dashboard listens on `127.0.0.1` only. Configuration is saved outside the npm package and is retained on removal.

# AI installation prompt

[简体中文](ai-install-prompt.zh-CN.md) | English

Copy the prompt below into your coding agent (Claude Code, Cursor, etc.). It
installs photoSelf on a Linux host, NAS, or OpenWrt device.

The agent should **ask first**, keep the photo library read-only, and verify the
installation before reporting back.

```text
You are installing photoSelf, a lightweight self-hosted photo/video gallery:
https://github.com/shinvdu/photoSelf

Read docs/installation.md and docs/usage.md first. Follow them; do not invent
paths or commands.

Before making changes, ask me for:
1. Install target: source build, Docker, or OpenWrt/ARM cross-compile.
2. Absolute path(s) of my photo library (one or more). These must remain
   read-only; never move, rename, chmod, or delete anything inside them.
3. A writable data directory for SQLite, thumbnails and temporary files.
4. Network: LAN bind address/port, or whether a reverse proxy (Nginx/Caddy/
   lighttpd) will terminate TLS and authentication.
5. Authentication: "basic" (single admin, recommended), "proxy" (trusted
   reverse proxy header), or "none" (trusted LAN only). For basic auth, ask me
   for a password and generate the Argon2id hash with:
   photoself hash-password --password '...'
6. Whether the library contains HEIC or HEVC MOV files. If yes, explain that
   low-power devices may need sidecar previews generated on a PC/NAS with
   scripts/make-sidecars.py.
7. Whether any system packages may be installed (Rust, Node, ffmpeg, zig,
   libheif-examples). Ask before installing them.

Then:
- Back up any existing config before overwriting it.
- Build the UI and binary, or use Docker as I chose.
- Install the service using the provided systemd/procd/Docker assets.
- Start the service and verify:
  * curl http://127.0.0.1:8787/healthz returns {"status":"ok"}
  * the UI responds at the chosen URL
  * an initial scan job completes (report indexed/error/missing counts)
  * fetch one thumbnail and one media Range request
- Report: install path, service name, URL, data directory, library IDs, auth
  username (not the password), password hash location, scan results, and any
  warnings.

Safety rules:
- Never expose the app directly to the public internet without TLS and auth.
- Do not run destructive commands or recursively delete anything.
- Do not modify the photo files during scan, preview generation, or testing.
- Prefer read-only mounts for photo libraries.
- If a step fails, show the exact error and stop rather than guessing.
```

# Security

photoSelf is a self-hosted web application that can read your photo library.
Treat it like any other file-serving service.

## Supported versions

Only the latest 0.1.x MVP is supported while the project is pre-1.0.

## Reporting a vulnerability

Please open a private security advisory on GitHub (Security → Advisories) or
contact the maintainers privately. Do not open a public issue with exploit
details. Include the version, deployment mode and reproduction steps.

## Deployment guidance

- Never expose the app directly to the internet without TLS and authentication.
  Use a reverse proxy (Nginx/Caddy/lighttpd) or a VPN/WireGuard.
- `auth.mode = "none"` is for a trusted LAN only.
- `auth.mode = "proxy"` trusts `X-Forwarded-User`; only enable it behind a proxy
  that strips client-supplied values and authenticates users.
- Use `auth.mode = "basic"` with an Argon2id hash from
  `photoself hash-password --password '...'` if you do not have a proxy.
- Mount the photo library read-only for the service account when possible.
- Keep `data.dir` writable only by the service user.
- Do not enable the (future) command/exec features of third-party tools.

## Known limitations

- No built-in TLS; terminate TLS at the reverse proxy.
- Rate limiting is delegated to the reverse proxy.
- Video and image parsing is delegated to `ffmpeg`/`image`; a malicious file may
  consume CPU/memory. Thumbnail concurrency is limited by configuration.
- SQLite is bundled; keep regular backups of `data.dir` if user metadata matters.

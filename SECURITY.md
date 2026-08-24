# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.x (latest) | ✅ Security fixes |

## Reporting a Vulnerability

**Please do NOT open a public issue for security vulnerabilities.**

Report via GitHub Security Advisories:
https://github.com/eddypepy/ubuntu-miracast-client-rs/security/advisories/new

### Response Timeline

| Action | Timeline |
|--------|----------|
| Acknowledgment | Within 48 hours |
| Initial assessment | Within 7 days |
| Fix development | Within 30 days (critical) |

## Security Design

This project follows defense-in-depth:

1. **No subprocess calls** — All system interaction via typed D-Bus APIs (no shell injection surface)
2. **No root/sudo** — Uses D-Bus (polkit-mediated) and Linux capabilities
3. **Buffer bounds** — All network parsing has `MAX_RECV_BUFFER` (64KB) and `MAX_CONTENT_LENGTH` (16KB) limits
4. **Type safety** — Rust's ownership system prevents memory corruption, use-after-free, buffer overflows
5. **Input validation** — WFD hex parsing validates character set, length, subelement ID, port range
6. **Dependency auditing** — `cargo audit` + `cargo deny` in CI
7. **No unsafe code** — Zero `unsafe` blocks in application code

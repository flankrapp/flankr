# flankr-daemon versions

## 0.2.0 — 2026-11-14 (this commit, tag v0.2.0)

Adds the additive platform-neutral persistent user-service API: `ServiceSpec`,
`ServiceStatus`, `UserServiceBackend`, the macOS per-user launchd backend, and
an explicit unsupported-platform backend. Existing `Detached` one-shot start
behavior is unchanged.

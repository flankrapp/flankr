# flankr-daemon versions

## 0.2.0 — 2026-11-14 (this commit, tag v0.2.0)

Adds the additive platform-neutral persistent user-service API: `ServiceSpec`,
`ServiceStatus`, `UserServiceBackend`, the macOS per-user launchd backend, and
an explicit unsupported-platform backend. Existing `Detached` one-shot start
behavior is unchanged.

## Correction — 2026-11-14 21:07 EST: existing v0.2.0 tag

The `v0.2.0` tag name already belonged to an earlier root flankr release and
was not moved or deleted. The 0.2.0 entry above therefore records the daemon
release content, but its parenthetical tag claim is not an assertion that this
implementation moved that existing tag.

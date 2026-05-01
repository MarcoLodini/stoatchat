# Stoat Chat (Backend) — AGENTS.md

Self-hosted chat platform (Discord alternative). A fork of [Revolt Chat](https://github.com/revoltchat) by
[stoatchat](https://github.com/stoatchat), customized with Authentik OIDC SSO.

## Architecture

**Backend** — Rust monorepo, Rocket web framework. MongoDB, Redis (KeyDB), RabbitMQ, MinIO.

## SSO flow (end-to-end)

```
User clicks "Log In with SSO"
  → GET /api/auth/sso/login
  → 302 → Authentik OIDC authorize
  → User authenticates on Authentik
  → Authentik redirects → GET /api/auth/sso/callback?code=...
  → exchanges code for token, fetches userinfo
  → creates/finds account in MongoDB, creates session
  → 302 → /login/sso?token=SESSION_TOKEN
```

## SSO code map

All in `crates/delta/src/routes/sso/`:
- `discovery.rs` — fetches `.well-known/openid-configuration`, cached in `OnceCell` (lifetime: process)
- `login.rs` — builds OIDC authorize URL with hardcoded scopes `openid email profile`
- `callback.rs` — exchanges code, creates/looks-up account, creates user, creates session
- `mod.rs` — route dispatcher, `GET /login`, `GET /callback`, `GET /end-session`

**Config** (`crates/core/config/src/lib.rs`):
- `struct Sso` — `enabled`, `issuer_url`, `client_id`, `client_secret`, `redirect_uri`
- Env vars: `REVOLT__SSO__ENABLED`, `REVOLT__SSO__ISSUER_URL`, etc.

**Route registration** (`crates/delta/src/routes/mod.rs`):
- `mod sso;` mounted as `"/auth/sso"` in 4 route blocks
- `POST /auth/session/login` filtered at mount-time when `config.sso.enabled` is true

**Dependencies**: Zero new Rust crates. SSO reuses existing workspace deps (`reqwest`, `nanoid`, `once_cell`, `serde`, `url-escape`). PKCE added `base64`, `sha2` workspace deps.

## Environment variables

| Variable | Description |
|----------|-------------|
| `REVOLT__SSO__ENABLED` | `true` to enable SSO |
| `REVOLT__SSO__ISSUER_URL` | Authentik issuer, e.g. `https://authentik.saildot.it/application/o/stoat/` |
| `REVOLT__SSO__CLIENT_ID` | OIDC client ID |
| `REVOLT__SSO__CLIENT_SECRET` | OIDC client secret |
| `REVOLT__SSO__REDIRECT_URI` | Callback URL, e.g. `https://stoat.saildot.it/api/auth/sso/callback` |

## Fork maintenance strategy

This is a fork of [stoatchat/stoatchat](https://github.com/stoatchat/stoatchat). Upstream releases
are tagged as `vX.Y.Z` on GitHub. The customizations are:

### Files modified by us (conflict surface)

| File | Risk | Notes |
|------|------|-------|
| `crates/delta/src/routes/mod.rs` | Low | +5 lines, clean addition of `mod sso;` and 4 mount lines |
| `crates/core/config/src/lib.rs` | Medium | +20 lines. `Sso` struct + field + preflight check |
| `crates/delta/Cargo.toml` | Low | +3 dependency lines (url-escape, base64, sha2) |

### Files added by us (no upstream conflict)

- `crates/delta/src/routes/sso/*` (5 files, ~250 lines)

### Recommended strategy

**Primary approach — periodic rebase on upstream tags:**

1. Add `upstream` remote: `git remote add upstream https://github.com/stoatchat/stoatchat.git`
2. On each upstream release: `git fetch upstream && git rebase upstream/vX.Y.Z`
3. Resolve 0-2 low/medium-risk config/router conflicts (trivial one-liners)

**Ideal — upstream the SSO changes:**
The backend SSO code is clean, additive, and zero-dependency. Consider submitting a PR to upstream
stoatchat that adds OIDC SSO as an optional feature (behind `sso.enabled` config flag). If accepted,
our fork shrinks to just the frontend changes. The stoatchat project already has a
contribution guide at https://developers.stoat.chat/developing/contrib/.

## Known gaps

1. ✅ Block email/password login — `POST /api/auth/session/login` filtered at mount-time in all 4 route blocks when `config.sso.enabled` is true.
2. ✅ PKCE — SHA-256 challenge (`code_challenge_method=S256`), in-memory state→verifier store with 10-min TTL, CSRF via `state` parameter.
3. ✅ No logout sync — RP-Initiated Logout (`end_session.rs`) redirects to Authentik's `end_session_endpoint`.
4. ✅ Opaque SSO errors — user-friendly error redirects (`/login?error=sso_error` or `...?error=sso_disabled`).
5. OIDC discovery cached permanently — `OnceCell` in `discovery.rs`, never refreshed. Requires restart if issuer config changes.

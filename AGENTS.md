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

## SSO Exclusive Mode (IdP-managed accounts)

When `config.sso.enabled` is true, SSO is **always exclusive** — no mixed local/SSO users.
All local account management is disabled because the IdP is the single source of truth.

### User tracking (`UserFlags::Sso = 16`)
- `crates/core/models/src/v0/users.rs` — `UserFlags::Sso` added to bitflags enum
- `crates/delta/src/routes/sso/callback.rs` — sets `Sso` flag on new user creation and updates existing users on SSO login
- Exposed in API via `user.flags`; frontend checks `(flags & 16) !== 0`

### Blocked routes (filtered at mount-time in `crates/delta/src/routes/mod.rs`)
All authifier account routes are removed except `GET /auth/account/` (fetch account info):
- Account creation, deletion, disable
- Password change, password reset
- Email change, email verification, resend verification

### Blocked operations for SSO users
- `PATCH /users/@me/username` — returns `InvalidOperation` if `UserFlags::Sso` is set

## Known gaps

1. ✅ Block email/password login — `POST /api/auth/session/login` filtered at mount-time in all 4 route blocks when `config.sso.enabled` is true.
2. ✅ PKCE — SHA-256 challenge (`code_challenge_method=S256`), in-memory state→verifier store with 10-min TTL, CSRF via `state` parameter.
3. ✅ No logout sync — RP-Initiated Logout (`end_session.rs`) redirects to Authentik's `end_session_endpoint`.
4. ✅ Opaque SSO errors — user-friendly error redirects (`/login?error=sso_error` or `...?error=sso_disabled`).
5. ✅ SSO exclusive mode — all local account management disabled, `UserFlags::Sso` tracks IdP users.
6. OIDC discovery cached permanently — `OnceCell` in `discovery.rs`, never refreshed. Requires restart if issuer config changes.

## Known upstream issues (not our bugs)

1. **MongoDB test stack overflow** — `bots::crud` and `users::create_user` tests crash with
   `thread has overflowed its stack` / `fatal runtime error: stack overflow` in test profile.
   This happens on upstream revoltchat/stoatchat too. Not caused by our SSO changes.

## CI notes

1. **Docker publishes on tag push only** — trigger is `push: tags: - "*"`. Not on merge to main.
   Retrigger: `git tag -f latest && git push -f origin latest`. The `concurrency` group cancels
   in-flight builds on re-tag.
2. **Cargo.lock must be regenerated after adding workspace deps** — Docker builds use `--locked`.
   Run `cargo generate-lockfile` after
   modifying the root `Cargo.toml` workspace dependencies.
3. **GHCR image tags must be lowercase** — `github.repository_owner` preserves case.
   Fixed via bash `${OWNER,,}` step in `docker.yaml`. GitHub Actions has no built-in lowercase
   filter.
4. **Docker action version lock-in** — `docker/build-push-action@v4` (2023) hangs silently with
   zero output on Docker 28.0.4 / buildx 0.33.0 (2025+). Keep actions current. Tested working
   set: `build-push-action@v6`, `setup-buildx-action@v3`, `login-action@v3`,
   `metadata-action@v5`, `checkout@v4`.
5. **`cargo build` parallelism in Docker** — don't set `-j` higher than `$(nproc)`. `-j 10`
   on 4 CPUs causes CPU thrashing and OOM on the 15.6 GiB runner. Use `cargo build -j $(nproc)`
   or omit `-j` entirely.
6. **`latest` tag is protected** — ruleset #15848214 blocks deletion and force-push to
   `refs/tags/latest`. Admin bypass only. Prevents unauthorized Docker publishes since the
   tag triggers the CI workflow.

# Feature Flag Service

> A production-ready feature flag management service built with Rust, Axum, and PostgreSQL

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/Webrowse/feature-flag-service-backend/actions/workflows/ci.yml/badge.svg)](https://github.com/Webrowse/feature-flag-service-backend/actions/workflows/ci.yml)

**[Complete API Documentation](./API.md)** | **[Postman Collection](./postman_collection.json)**

## What Is This?

A complete, self-hosted feature flag service that lets you control feature rollouts, run A/B tests, and target specific users without touching your deployment pipeline. It provides a management API for developers and an SDK endpoint for client applications to evaluate flags in real-time.

**Key Features:**

- **Feature Flag Management**: Create, update, and toggle flags across projects and environments
- **Targeting Rules**: Target users by ID, email address, or email domain
- **Percentage Rollouts**: Gradual rollouts with consistent, stable hashing (same user always gets the same result)
- **Multi-Environment Support**: Production, staging, and custom environments per project
- **SDK Integration**: Lightweight SDK endpoint authenticated with an API key
- **Analytics**: Every evaluation is logged for dashboards and reporting
- **Rate Limiting**: IP-based rate limiting on auth and SDK endpoints to prevent abuse
- **Secure**: JWT auth, Argon2 password hashing, user-scoped data access, no secrets in responses

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                  Feature Flag Service                   │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Management API (/api/*)     SDK API (/sdk/*)           │
│  JWT Authentication          SDK Key Authentication     │
│  ├── Projects                ├── Evaluate all flags     │
│  ├── Environments            └── for a given user       │
│  ├── Feature Flags                                      │
│  └── Targeting Rules                                    │
│                                                         │
│  ┌───────────────────────────────────────────┐          │
│  │         Evaluation Engine                 │          │
│  │  1. Global enabled/disabled check         │          │
│  │  2. Targeting rules (priority order)      │          │
│  │  3. Percentage rollout (FNV-1a hash)      │          │
│  │  4. Async evaluation logging              │          │
│  └───────────────────────────────────────────┘          │
│                                                         │
│  ┌───────────────────────────────────────────┐          │
│  │         PostgreSQL Database               │          │
│  │  users · projects · environments          │          │
│  │  feature_flags · flag_rules               │          │
│  │  flag_evaluations                         │          │
│  └───────────────────────────────────────────┘          │
└─────────────────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- **Rust** 1.88+ ([Install](https://www.rust-lang.org/tools/install))
- **Docker** ([Install](https://docs.docker.com/get-docker/))

### Running Locally

```bash
git clone git@github.com:Webrowse/feature-flag-service-backend.git
cd feature-flag-service-backend
cp .env.example .env
# set a secure JWT_SECRET in .env

docker-compose up -d
cargo install sqlx-cli --no-default-features --features postgres
sqlx migrate run
cargo run
```

The service starts on `http://0.0.0.0:8080`.

### How it works

1. Register and log in to get a JWT for the management API.
2. Create a project to get an SDK key. Production and staging environments are created for you automatically.
3. Create a feature flag inside an environment. Set a rollout percentage, add targeting rules, or leave it as a simple on/off switch.
4. Call `/sdk/v1/evaluate` from your application with the SDK key and a user context. The service returns the current state of every flag for that user in one response.

For full request and response shapes, see [API.md](./API.md). A [Postman collection](./postman_collection.json) is included that automatically saves tokens and IDs across requests.

---

## Core Concepts

### 1. Projects

A project is the top-level container for everything in the service. It represents one of your applications. Each project has a unique SDK key that your application uses to call the evaluation endpoint. When you create a project, the service automatically sets up production and staging environments so you can start using it immediately.

From a project you can: create additional environments, rotate the SDK key if it is ever compromised, update the project name, or delete it. Deleting a project removes all its environments, flags, rules, and evaluation history.

### 2. Environments

Environments let you run the same set of flags with different configurations in production, staging, or any other context you need. A flag enabled in staging has no effect on production. Every flag lives inside exactly one environment.

Environment keys must be lowercase letters, numbers, `_`, or `-`. You can add as many environments as you need beyond the default production and staging ones.

### 3. Feature Flags

A flag is a named boolean switch that lives inside an environment. Each flag has a key that your application code references (for example, `dark_mode` or `new_checkout`). You control it in three ways:

- **Global switch**: turn the flag completely off for everyone, regardless of rules or rollout
- **Targeting rules**: return true for specific users immediately, before any rollout logic runs
- **Rollout percentage**: gradually expose the flag to a percentage of users in a consistent, stable way

Flag keys must be lowercase letters, numbers, `_`, or `-`, starting with a letter.

### 4. Targeting Rules

Targeting rules let you enable a flag for specific users without affecting everyone else. Rules are evaluated in priority order (highest number first), and the first matching rule ends evaluation with a result of `true`.

| `rule_type` | `rule_value` example | Matches when |
|---|---|---|
| `user_id` | `"user_12345"` | context.user_id equals the value |
| `user_email` | `"alice@example.com"` | context.user_email equals the value |
| `email_domain` | `"@company.com"` | context.user_email ends with this domain |

Rules run before the rollout percentage. A user who matches a rule always gets `true`, even if the rollout is set to 0%.

### 5. Flag Evaluation

The evaluation algorithm in order:

1. **Global switch**: if `enabled = false`, return `false` immediately
2. **Targeting rules**: evaluate enabled rules, highest priority first; first match returns `true`
3. **Percentage rollout**: hash `flag_key:user_identifier` with FNV-1a; return `true` if bucket < rollout %
4. **Default**: if `enabled = true` and no rollout set, return `true` for everyone

**Evaluate all flags for a user:**

```bash
POST /sdk/v1/evaluate
X-SDK-Key: sdk_abc123...
Content-Type: application/json

{
  "environment": "production",
  "context": {
    "user_id": "user_42",
    "user_email": "alice@example.com"
  }
}
```

```json
{
  "flags": {
    "dark_mode": {
      "enabled": true,
      "reason": "User in 50% rollout"
    },
    "new_checkout": {
      "enabled": true,
      "reason": "Matched email_domain targeting rule"
    },
    "premium_features": {
      "enabled": false,
      "reason": "Flag is globally disabled"
    }
  }
}
```

---

## API Overview

### Public Endpoints

| Method | Endpoint | Description |
|---|---|---|
| GET | `/health` | Health check (includes DB ping) |
| POST | `/auth/register` | Register new user |
| POST | `/auth/login` | Login, receive JWT |

### Management API (`Authorization: Bearer <token>` required)

**Projects**

| Method | Endpoint | Description |
|---|---|---|
| POST | `/api/projects` | Create project |
| GET | `/api/projects` | List your projects |
| GET | `/api/projects/{id}` | Get project |
| PUT | `/api/projects/{id}` | Update project |
| DELETE | `/api/projects/{id}` | Delete project |
| POST | `/api/projects/{id}/regenerate-key` | Rotate SDK key |

**Environments**

| Method | Endpoint | Description |
|---|---|---|
| POST | `/api/projects/{pid}/environments` | Create environment |
| GET | `/api/projects/{pid}/environments` | List environments |
| GET | `/api/projects/{pid}/environments/{eid}` | Get environment |
| PUT | `/api/projects/{pid}/environments/{eid}` | Update environment |
| DELETE | `/api/projects/{pid}/environments/{eid}` | Delete environment |

**Feature Flags**

| Method | Endpoint | Description |
|---|---|---|
| POST | `/api/projects/{pid}/environments/{eid}/flags` | Create flag |
| GET | `/api/projects/{pid}/environments/{eid}/flags` | List flags |
| GET | `/api/projects/{pid}/environments/{eid}/flags/{fid}` | Get flag |
| PUT | `/api/projects/{pid}/environments/{eid}/flags/{fid}` | Update flag |
| DELETE | `/api/projects/{pid}/environments/{eid}/flags/{fid}` | Delete flag |
| POST | `/api/projects/{pid}/environments/{eid}/flags/{fid}/toggle` | Toggle on/off |

**Targeting Rules**

| Method | Endpoint | Description |
|---|---|---|
| POST | `.../flags/{fid}/rules` | Create rule |
| GET | `.../flags/{fid}/rules` | List rules |
| GET | `.../flags/{fid}/rules/{rid}` | Get rule |
| PUT | `.../flags/{fid}/rules/{rid}` | Update rule |
| DELETE | `.../flags/{fid}/rules/{rid}` | Delete rule |

### SDK API (`X-SDK-Key: sdk_...` required)

| Method | Endpoint | Description |
|---|---|---|
| POST | `/sdk/v1/evaluate` | Evaluate all flags for a user |

See [API.md](./API.md) for complete request/response documentation.

---

## Project Structure

```
feature-flag-service-backend/
├── .github/
│   └── workflows/
│       └── ci.yml                 # CI (fmt, clippy, test) + CD (Docker → Railway)
│
├── migrations/
│   ├── 20251130132949_create_users.sql
│   ├── 20251130132951_feature_flag.sql
│   ├── 20251225000000_create_environments.sql
│   └── 20260101000000_production_fixes.sql
│
├── src/
│   ├── main.rs                    # Startup: pool, CORS, timeout, graceful shutdown
│   ├── config.rs                  # Env var loading and validation
│   ├── state.rs                   # AppState (db pool + jwt_secret)
│   │
│   ├── evaluation/
│   │   └── mod.rs                 # Evaluation engine + unit tests
│   │
│   └── routes/
│       ├── mod.rs                 # Router wiring
│       ├── auth.rs                # Register, login
│       ├── health.rs              # Health check with DB ping
│       ├── middleware_auth.rs     # JWT middleware + JwtUser extractor
│       ├── rate_limit.rs          # IP-based rate limiting middleware
│       ├── sdk_auth.rs            # SDK key middleware + SdkProject extractor
│       │
│       ├── projects/
│       ├── environments/
│       ├── flags/
│       ├── rules/
│       └── sdk/
│
├── Dockerfile                     # Multi-stage production image
├── docker-compose.yml             # Local Postgres
├── Cargo.toml
└── .env.example
```

---

## Database Schema

| Table | Purpose |
|---|---|
| `users` | Auth: email + Argon2 password hash |
| `projects` | Top-level grouping; holds the SDK key |
| `environments` | Scopes flags (production, staging, etc.) |
| `feature_flags` | Flag config per environment |
| `flag_rules` | Targeting rules per flag |
| `flag_evaluations` | Evaluation log for analytics |

**Key constraints:**
- Flag keys are unique per environment
- `flag_rules.rule_type` is DB-enforced to `user_id | user_email | email_domain`
- Deleting a project cascades to environments → flags → rules → evaluations

---

## Tech Stack

| Component | Technology |
|---|---|
| Language | Rust 1.88+ |
| Web framework | Axum 0.8 |
| Database | PostgreSQL 16 |
| DB driver | SQLx 0.8 (compile-time verified queries) |
| Async runtime | Tokio |
| Auth | JWT (jsonwebtoken 9), Argon2 |
| Rate limiting | governor 0.6 |
| Observability | tracing + tracing-subscriber |
| CORS / Timeout | tower-http |

---

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | Yes | none | Postgres connection string |
| `JWT_SECRET` | Yes | none | Signing key, minimum 32 characters |
| `PORT` | Yes | none | Port to listen on |
| `HOST` | No | `0.0.0.0` | Bind address |
| `ALLOWED_ORIGIN` | No | `http://localhost:3000` | CORS allowed origin. Comma-separated for multiple origins. Use `*` to allow all origins. |
| `RUST_LOG` | No | `info` | Log level |

Generate a secure JWT secret:
```bash
openssl rand -base64 48
```

---

## Development

```bash
# Run tests (15 unit tests, no DB required)
cargo test

# Lint
cargo clippy --all-targets -- -D warnings

# Format
cargo fmt

# Add a migration
sqlx migrate add my_migration_name

# Regenerate sqlx offline cache after schema changes
cargo sqlx prepare
```

---

## Deploying

The project ships with a multi-stage Dockerfile. Push to GitHub and connect your repo to Railway: it detects the Dockerfile and builds automatically. Add a PostgreSQL database from the Railway dashboard, set the required environment variables, and run migrations once via the Railway shell.

Required variables: `DATABASE_URL`, `JWT_SECRET`, `PORT`, `ALLOWED_ORIGIN`.

The CI/CD workflow in `.github/workflows/ci.yml` runs fmt, clippy, and tests on every push, then builds and pushes a Docker image on merge to master.

---

## Use Cases

### Gradual rollout

Create a flag with `rollout_percentage` set to 10. Users are assigned to a bucket using a deterministic hash of their user ID and the flag key, so the same user always gets the same result across requests. When you are confident in the change, update the percentage to 50, then 100. No redeployment needed at any step.

### Internal beta

Add an `email_domain` rule for `@yourcompany.com`. All employees match the rule and get the flag enabled immediately, regardless of the rollout percentage. Everyone else is subject to the normal rollout or sees the flag as disabled.

### VIP early access

Add a `user_email` rule for a specific account. That user gets access right away. You can stack multiple rules at different priority levels to handle complex targeting without touching code.

### Emergency kill switch

Toggle a flag off by calling the toggle endpoint. The change takes effect on the next evaluation call. No deployment, no config change, no incident required.

---

## Performance

- Flag evaluation typically completes in **< 10ms** end-to-end (two DB queries: one for flags, one batch for all rules)
- Evaluation logs are written **asynchronously** (`tokio::spawn`) and never block the response
- DB pool is capped at **20 connections** with a 30-second acquire timeout
- All requests time out after **30 seconds**
- Rollout hashing uses FNV-1a, deterministic across deployments, O(1) per user

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

## License

MIT. See LICENSE file.

## Support

- **Issues**: [GitHub Issues](https://github.com/Webrowse/feature-flag-service-backend/issues)
- **API Reference**: [API.md](./API.md)

---

Built with Rust.

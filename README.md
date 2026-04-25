# Feature Flag Service

> A production-ready feature flag management service built with Rust, Axum, and PostgreSQL

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.82%2B-orange.svg)](https://www.rust-lang.org/)
[![CI](https://github.com/Webrowse/feature-flag-service-backend/actions/workflows/ci.yml/badge.svg)](https://github.com/Webrowse/feature-flag-service-backend/actions/workflows/ci.yml)

**[📖 Complete API Documentation →](./API.md)** | **[🧪 Postman Collection →](./postman_collection.json)**

## What Is This?

A complete, self-hosted feature flag service that lets you control feature rollouts, run A/B tests, and target specific users — without touching your deployment pipeline. It provides a management API for developers and an SDK endpoint for client applications to evaluate flags in real-time.

**Key Features:**

- **Feature Flag Management** — Create, update, and toggle flags across projects and environments
- **Targeting Rules** — Target users by ID, email address, or email domain
- **Percentage Rollouts** — Gradual rollouts with consistent, stable hashing (same user always gets the same result)
- **Multi-Environment Support** — Production, staging, and custom environments per project
- **SDK Integration** — Lightweight SDK endpoint authenticated with an API key
- **Analytics** — Every evaluation is logged for dashboards and reporting
- **Secure** — JWT auth, Argon2 password hashing, user-scoped data access, no secrets in responses

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

- **Rust** 1.82+ ([Install](https://www.rust-lang.org/tools/install))
- **Docker** ([Install](https://docs.docker.com/get-docker/))

### 5 Steps to Running Locally

```bash
# 1. Clone the repository
git clone git@github.com:Webrowse/feature-flag-service-backend.git
cd feature-flag-service-backend

# 2. Set up environment variables
cp .env.example .env
# Edit .env — at minimum set a secure JWT_SECRET:
#   openssl rand -base64 48

# 3. Start PostgreSQL
docker-compose up -d

# 4. Run database migrations
cargo install sqlx-cli --no-default-features --features postgres
sqlx migrate run

# 5. Run the service
cargo run
# → Listening on http://0.0.0.0:8080
```

### Quick Test (curl)

```bash
BASE=http://localhost:8080

# Register
curl -s -X POST $BASE/auth/register \
  -H "Content-Type: application/json" \
  -d '{"email":"dev@example.com","password":"secure123"}'

# Login — save the token
TOKEN=$(curl -s -X POST $BASE/auth/login \
  -H "Content-Type: application/json" \
  -d '{"email":"dev@example.com","password":"secure123"}' | jq -r '.token')

# Create a project (also creates production + staging environments)
PROJECT=$(curl -s -X POST $BASE/api/projects \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"My App"}' | jq -r '.id')

SDK_KEY=$(curl -s $BASE/api/projects/$PROJECT \
  -H "Authorization: Bearer $TOKEN" | jq -r '.sdk_key')

# Get the production environment ID
ENV_ID=$(curl -s $BASE/api/projects/$PROJECT/environments \
  -H "Authorization: Bearer $TOKEN" | jq -r '[.[] | select(.key=="production")][0].id')

# Create a feature flag inside that environment
FLAG=$(curl -s -X POST $BASE/api/projects/$PROJECT/environments/$ENV_ID/flags \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name":"Dark Mode","key":"dark_mode","enabled":true,"rollout_percentage":50}' \
  | jq -r '.id')

# Evaluate flags via SDK
curl -s -X POST $BASE/sdk/v1/evaluate \
  -H "X-SDK-Key: $SDK_KEY" \
  -H "Content-Type: application/json" \
  -d '{"environment":"production","context":{"user_id":"user_42"}}'
```

### Using Postman

Import [postman_collection.json](./postman_collection.json) for a pre-built collection that automatically saves tokens, IDs, and SDK keys across requests.

---

## Core Concepts

### 1. Projects

A project represents one of your applications. Creating a project automatically creates **production** and **staging** environments.

```bash
POST /api/projects
{"name": "Mobile App", "description": "iOS and Android"}

# Response includes sdk_key — used by your app to call /sdk/v1/evaluate
```

### 2. Environments

Environments scope flags so production and staging can have independent states. Every flag lives inside an environment.

```bash
GET  /api/projects/{project_id}/environments
POST /api/projects/{project_id}/environments
{"name": "Canary", "key": "canary"}
```

Environment keys must be lowercase letters, numbers, `_`, or `-`.

### 3. Feature Flags

A flag is a boolean switch that can target specific users or roll out gradually.

```bash
POST /api/projects/{project_id}/environments/{environment_id}/flags
{
  "name": "New Checkout",
  "key": "new_checkout",
  "enabled": true,
  "rollout_percentage": 25
}
```

Flag keys must be lowercase letters, numbers, `_`, or `-`, starting with a letter.

### 4. Targeting Rules

Rules evaluate before the rollout percentage. A matching rule immediately returns `true`, regardless of rollout. Rules run in **priority order** — highest number first.

| `rule_type` | `rule_value` example | Matches when |
|---|---|---|
| `user_id` | `"user_12345"` | context.user_id equals value |
| `user_email` | `"alice@example.com"` | context.user_email equals value |
| `email_domain` | `"@company.com"` | email ends with this domain |

```bash
POST /api/projects/{project_id}/environments/{environment_id}/flags/{flag_id}/rules
{
  "rule_type": "email_domain",
  "rule_value": "@yourcompany.com",
  "priority": 100,
  "enabled": true
}
```

### 5. Flag Evaluation

The evaluation algorithm in order:

1. **Global switch** — if `enabled = false`, return `false` immediately
2. **Targeting rules** — evaluate enabled rules, highest priority first; first match returns `true`
3. **Percentage rollout** — hash `flag_key:user_identifier` with FNV-1a; return `true` if bucket < rollout %
4. **Default** — if `enabled = true` and no rollout set, return `true` for everyone

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

### Management API — `Authorization: Bearer <token>` required

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

### SDK API — `X-SDK-Key: sdk_...` required

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
| `users` | Auth — email + Argon2 password hash |
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
| Language | Rust 1.82+ |
| Web framework | Axum 0.8 |
| Database | PostgreSQL 16 |
| DB driver | SQLx 0.8 (compile-time verified queries) |
| Async runtime | Tokio |
| Auth | JWT (jsonwebtoken 9), Argon2 |
| Observability | tracing + tracing-subscriber |
| CORS / Timeout | tower-http |

---

## Environment Variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | Yes | — | Postgres connection string |
| `JWT_SECRET` | Yes | — | Signing key, minimum 32 characters |
| `PORT` | Yes | — | Port to listen on |
| `HOST` | No | `0.0.0.0` | Bind address |
| `ALLOWED_ORIGIN` | No | `http://localhost:3000` | CORS allowed origin |
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

## Production Deployment

### Docker (manual)

```bash
docker build -t feature-flag-service .

docker run -d \
  -e DATABASE_URL="postgres://..." \
  -e JWT_SECRET="..." \
  -e ALLOWED_ORIGIN="https://your-frontend.com" \
  -e PORT=8080 \
  -p 8080:8080 \
  feature-flag-service
```

### Railway (recommended)

1. Push to GitHub
2. New project → Deploy from GitHub repo (Railway detects the Dockerfile)
3. Add a PostgreSQL database from the Railway dashboard
4. Set `JWT_SECRET`, `ALLOWED_ORIGIN`, `PORT=8080` in Variables
5. Run migrations via the Railway shell: `sqlx migrate run`

The CI/CD workflow in `.github/workflows/ci.yml` runs tests on every push and deploys automatically on merge to `master`.

---

## Use Cases

### Gradual rollout

```bash
# Start at 10%, increase over time
curl -X POST .../flags \
  -d '{"name":"New UI","key":"new_ui","enabled":true,"rollout_percentage":10}'

curl -X PUT .../flags/$FLAG_ID \
  -d '{"rollout_percentage":50}'

curl -X PUT .../flags/$FLAG_ID \
  -d '{"rollout_percentage":100}'
```

### Internal beta (company email domain)

```bash
curl -X POST .../flags/$FLAG_ID/rules \
  -d '{"rule_type":"email_domain","rule_value":"@yourcompany.com","priority":100}'
```

### VIP early access

```bash
curl -X POST .../flags/$FLAG_ID/rules \
  -d '{"rule_type":"user_email","rule_value":"vip@example.com","priority":100}'
```

### Emergency kill switch

```bash
curl -X POST .../flags/$FLAG_ID/toggle
```

---

## Client Integration

### JavaScript / TypeScript

```typescript
class FeatureFlagClient {
  constructor(private sdkKey: string, private baseUrl: string) {}

  async evaluate(environment: string, userId: string, userEmail?: string) {
    const res = await fetch(`${this.baseUrl}/sdk/v1/evaluate`, {
      method: 'POST',
      headers: {
        'X-SDK-Key': this.sdkKey,
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        environment,
        context: { user_id: userId, user_email: userEmail },
      }),
    });
    const data = await res.json();
    return data.flags as Record<string, { enabled: boolean; reason: string }>;
  }

  async isEnabled(flag: string, environment: string, userId: string) {
    const flags = await this.evaluate(environment, userId);
    return flags[flag]?.enabled ?? false;
  }
}

// Usage
const client = new FeatureFlagClient('sdk_abc123...', 'https://your-api.railway.app');

if (await client.isEnabled('dark_mode', 'production', 'user_42')) {
  enableDarkMode();
}
```

---

## Performance

- Flag evaluation typically completes in **< 10ms** end-to-end (two DB queries: one for flags, one batch for all rules)
- Evaluation logs are written **asynchronously** (`tokio::spawn`) and never block the response
- DB pool is capped at **20 connections** with a 5-second acquire timeout
- All requests time out after **30 seconds**
- Rollout hashing uses FNV-1a — deterministic across deployments, O(1) per user

---

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

## License

MIT — see LICENSE file.

## Support

- **Issues**: [GitHub Issues](https://github.com/Webrowse/feature-flag-service-backend/issues)
- **API Reference**: [API.md](./API.md)

---

Built with Rust 🦀

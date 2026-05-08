# Feature Flag Service - API Reference

Complete REST API documentation for managing feature flags and evaluating them in your applications.

**Base URL:** `http://localhost:3000`

> **Note:** For general information about the service, architecture, and deployment, see [README.md](./README.md)

## Table of Contents

- [Overview](#overview)
- [Authentication](#authentication)
- [API Endpoints](#endpoints)
  - [Health Check](#health-check)
  - [Authentication (Public)](#authentication-public)
  - [Current User](#current-user)
  - [Projects](#projects)
  - [Feature Flags](#feature-flags)
  - [Flag Rules (Targeting)](#flag-rules-targeting)
  - [SDK API](#sdk-api)
- [Error Responses](#error-responses)
- [Additional Resources](#additional-resources)

## Overview

This service provides two main API surfaces:

1. **Management API** (`/api/*`) - JWT authenticated endpoints for managing projects, flags, and rules
2. **SDK API** (`/sdk/*`) - SDK key authenticated endpoints for client applications to evaluate flags

## Authentication

### Management API

All `/api/*` endpoints require JWT authentication via the `Authorization: Bearer {token}` header.

**Obtaining a Token:**
1. Register a user account with `POST /auth/register`
2. Login with `POST /auth/login` to receive a JWT token
3. Include the token in the Authorization header for all subsequent requests

**Example:**
```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...
```

**Token Validity:** 24 hours

### SDK API

SDK endpoints use the `X-SDK-Key` header for authentication. SDK keys are generated when creating a project.

**Example:**
```http
X-SDK-Key: sdk_your_project_key_here
```

## Endpoints

### Health Check

#### Check Service Health
```
GET /health
Response: "OK"
```

### Authentication (Public)

#### Register

Create a new user account.

```http
POST /auth/register
Content-Type: application/json

{
  "email": "developer@example.com",
  "password": "secure_password_123"
}
```

**Response (201 Created):**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "email": "developer@example.com"
}
```

**Validation:**
- Email must be valid format and unique
- Password must be at least 8 characters
- Password is hashed with Argon2 before storage

#### Login

Authenticate and receive a JWT token.

```http
POST /auth/login
Content-Type: application/json

{
  "email": "developer@example.com",
  "password": "secure_password_123"
}
```

**Response (200 OK):**
```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
}
```

**Notes:**
- Token is valid for 24 hours
- Include token in `Authorization: Bearer {token}` header for all `/api/*` requests

#### Forgot Password

Request a password reset for an email address.

```http
POST /auth/forgot-password
Content-Type: application/json

{
  "email": "developer@example.com"
}
```

**Response (200 OK):**
```json
{
  "message": "If the account exists, a reset link has been sent."
}
```

**Notes:**
- Returns the same response regardless of whether the email exists.
- Reset tokens are single-use and expire after 30 minutes.

#### Reset Password

Reset account password using a valid reset token.

```http
POST /auth/reset-password
Content-Type: application/json

{
  "token": "reset_token_from_email",
  "new_password": "new_secure_password_123"
}
```

**Response (200 OK):**
```
Password reset successful
```

**Validation:**
- `new_password` must be at least 8 characters.
- Token must be valid, unexpired, and unused.

### Current User

#### Get Current User

Get the authenticated user's information.

```http
GET /api/me
Authorization: Bearer {token}
```

**Response (200 OK):**
```json
{
  "user_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

---

### Projects

#### Create Project

Create a new project to organize your feature flags.

```http
POST /api/projects
Authorization: Bearer {token}
Content-Type: application/json

{
  "name": "My Mobile App",
  "description": "iOS and Android application"
}
```

**Parameters:**
- `name` (string, required) - Project name
- `description` (string, optional) - Project description

**Response (201 Created):**
```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "name": "My Mobile App",
  "description": "iOS and Android application",
  "sdk_key": "sdk_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
  "created_at": "2024-12-14T10:00:00Z",
  "updated_at": "2024-12-14T10:00:00Z"
}
```

**Notes:**
- SDK key is automatically generated and globally unique
- Save the SDK key for client-side flag evaluation

#### List Projects

Get all projects owned by the authenticated user.

```http
GET /api/projects
Authorization: Bearer {token}
```

**Response (200 OK):**
```json
[
  {
    "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    "name": "My Mobile App",
    "description": "iOS and Android application",
    "sdk_key": "sdk_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
    "created_at": "2024-12-14T10:00:00Z",
    "updated_at": "2024-12-14T10:00:00Z"
  },
  {
    "id": "8d0f7780-8536-51ef-a055-f18gd2g01bf8",
    "name": "Web Dashboard",
    "description": "Admin dashboard",
    "sdk_key": "sdk_q1w2e3r4t5y6u7i8o9p0a1s2d3f4g5h6",
    "created_at": "2024-12-15T14:30:00Z",
    "updated_at": "2024-12-15T14:30:00Z"
  }
]
```

#### Get Project

Get details of a specific project.

```http
GET /api/projects/{project_id}
Authorization: Bearer {token}
```

**Response (200 OK):**
```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "name": "My Mobile App",
  "description": "iOS and Android application",
  "sdk_key": "sdk_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
  "created_at": "2024-12-14T10:00:00Z",
  "updated_at": "2024-12-14T10:00:00Z"
}
```

#### Update Project

Update project name or description.

```http
PUT /api/projects/{project_id}
Authorization: Bearer {token}
Content-Type: application/json

{
  "name": "My Mobile App (Production)",
  "description": "Production iOS and Android app"
}
```

**Parameters:**
- `name` (string, optional) - New project name
- `description` (string, optional) - New project description
- Only provided fields will be updated

**Response (200 OK):**
```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "name": "My Mobile App (Production)",
  "description": "Production iOS and Android app",
  "sdk_key": "sdk_a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6",
  "created_at": "2024-12-14T10:00:00Z",
  "updated_at": "2024-12-16T09:15:00Z"
}
```

#### Delete Project

Delete a project and all associated flags and rules.

```http
DELETE /api/projects/{project_id}
Authorization: Bearer {token}
```

**Response:** `204 No Content`

**Warning:** This action is irreversible and will cascade delete:
- All feature flags in the project
- All targeting rules for those flags
- All evaluation history

#### Regenerate SDK Key

Generate a new SDK key for a project. Use this if your SDK key has been compromised.

```http
POST /api/projects/{project_id}/regenerate-key
Authorization: Bearer {token}
```

**Response (200 OK):**
```json
{
  "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "name": "My Mobile App",
  "description": "iOS and Android application",
  "sdk_key": "sdk_z9y8x7w6v5u4t3s2r1q0p9o8n7m6l5k4",
  "created_at": "2024-12-14T10:00:00Z",
  "updated_at": "2024-12-16T10:00:00Z"
}
```

**Warning:** The old SDK key will be immediately invalidated. Update all client applications with the new key.

---

### Feature Flags

#### Create Flag
```
POST /api/projects/{project_id}/flags
Body: {
  "name": "New Checkout",
  "key": "new_checkout",              // lowercase, alphanumeric, _, -
  "description": "Optional",
  "enabled": true,                    // optional, default: false
  "rollout_percentage": 50           // optional, 0-100, default: 0
}
Response: {
  "id": "uuid",
  "project_id": "uuid",
  "name": "New Checkout",
  "key": "new_checkout",
  "description": "Optional",
  "enabled": true,
  "rollout_percentage": 50,
  "created_at": "2024-12-14T10:00:00Z",
  "updated_at": "2024-12-14T10:00:00Z"
}
```

**Validation Rules:**
- `key` must start with a letter
- `key` can only contain lowercase letters, numbers, `_`, and `-`
- `key` must be unique within the project
- `rollout_percentage` must be 0-100

#### List Flags
```
GET /api/projects/{project_id}/flags
Response: [ {...flag}, {...flag} ]
```

#### Get Flag
```
GET /api/projects/{project_id}/flags/{flag_id}
Response: {...flag}
```

#### Update Flag
```
PUT /api/projects/{project_id}/flags/{flag_id}
Body: {
  "name": "Updated Name",
  "description": "Updated description",
  "enabled": false,
  "rollout_percentage": 75
}
Note: All fields are optional, only provided fields are updated
Response: {...flag}
```

#### Toggle Flag
```
POST /api/projects/{project_id}/flags/{flag_id}/toggle
Response: {...flag with flipped enabled state}
```

#### Delete Flag
```
DELETE /api/projects/{project_id}/flags/{flag_id}
Response: 204 No Content
```

---

### Flag Rules (Targeting)

Target specific users or groups with advanced flag rules. Rules are evaluated for each flag to determine if it should be enabled for a specific user.

#### Create Rule
```
POST /api/projects/{project_id}/flags/{flag_id}/rules
Body: {
  "rule_type": "user_email",           // user_id, user_email, or email_domain
  "rule_value": "admin@example.com",   // The value to match
  "enabled": true,                     // optional, default: true
  "priority": 10                       // optional, default: 0, higher = evaluated first
}
Response: {
  "id": "uuid",
  "flag_id": "uuid",
  "rule_type": "user_email",
  "rule_value": "admin@example.com",
  "enabled": true,
  "priority": 10,
  "created_at": "2024-12-14T10:00:00Z"
}
```

**Rule Types:**
- `user_id` - Match specific user identifier
- `user_email` - Match specific email address (must contain @)
- `email_domain` - Match email domain (must start with @, e.g., "@company.com")

**Validation Rules:**
- `rule_value` cannot be empty
- Email domains must start with @
- User emails must contain @
- `priority` determines evaluation order (higher values evaluated first)

#### List Rules
```
GET /api/projects/{project_id}/flags/{flag_id}/rules
Response: [ {...rule}, {...rule} ]
Note: Rules are returned ordered by priority (highest first)
```

#### Get Rule
```
GET /api/projects/{project_id}/flags/{flag_id}/rules/{rule_id}
Response: {...rule}
```

#### Update Rule
```
PUT /api/projects/{project_id}/flags/{flag_id}/rules/{rule_id}
Body: {
  "rule_type": "email_domain",
  "rule_value": "@newcompany.com",
  "enabled": false,
  "priority": 20
}
Note: All fields are optional, only provided fields are updated
Response: {...rule}
```

#### Delete Rule
```
DELETE /api/projects/{project_id}/flags/{flag_id}/rules/{rule_id}
Response: 204 No Content
```

---

## Error Responses

All error responses follow this format:
```
Status: 4xx or 5xx
Body: "Error message string"
```

Common status codes:
- `400 Bad Request` - Invalid input (validation failed)
- `401 Unauthorized` - Missing or invalid JWT token
- `404 Not Found` - Resource doesn't exist
- `409 Conflict` - Duplicate key or other constraint violation
- `500 Internal Server Error` - Server-side error

---

## SDK API

The SDK API provides public endpoints for client applications to evaluate feature flags.

### Authentication

SDK endpoints use SDK key authentication via the `X-SDK-Key` header. SDK keys are generated when you create a project.

```
X-SDK-Key: sdk_your_key_here
```

### Evaluate Flags

Evaluate all feature flags for a given user context.

#### Request
```
POST /sdk/v1/evaluate
Headers:
  X-SDK-Key: sdk_your_project_key_here
  Content-Type: application/json

Body:
{
  "user_id": "user_12345",
  "user_email": "alice@example.com",
  "custom_attributes": {}
}
```

**Parameters:**
- `user_id` (string, required) - Unique identifier for the user
- `user_email` (string, optional) - User's email address for email-based targeting
- `custom_attributes` (object, optional) - Reserved for future custom attribute targeting

#### Response
```json
{
  "dark_mode": {
    "enabled": true,
    "reason": "rollout"
  },
  "new_checkout": {
    "enabled": true,
    "reason": "rule_match"
  },
  "premium_features": {
    "enabled": false,
    "reason": "disabled"
  }
}
```

**Response Format:**
- Returns an object where keys are flag keys
- Each flag has:
  - `enabled` (boolean) - Whether the flag is enabled for this user
  - `reason` (string) - Why the flag was enabled/disabled:
    - `"disabled"` - Flag is globally disabled
    - `"rule_match"` - User matched a targeting rule
    - `"rollout"` - User fell within the rollout percentage
    - `"rollout_excluded"` - User was excluded from rollout percentage

**Evaluation Algorithm:**
1. If flag is disabled → return `false` with reason `"disabled"`
2. Check targeting rules in priority order → return `true` with reason `"rule_match"` if matched
3. Apply percentage rollout with consistent hashing → return `true`/`false` with reason `"rollout"`/`"rollout_excluded"`

**Notes:**
- All evaluations are logged to the `flag_evaluations` table for analytics
- Consistent hashing ensures the same user always gets the same result for a given rollout percentage
- This endpoint is designed for high-throughput client-side evaluation

---

## Additional Resources

For more information about the service:
- **Architecture & Deployment**: See [README.md](./README.md)
- **Database Schema**: See [README.md](./README.md#database-schema)
- **Security Features**: See [README.md](./README.md#security-features)
- **Client Integration Examples**: See [README.md](./README.md#client-integration)
- **Testing**: Import [postman_collection.json](./postman_collection.json) for interactive API testing

# DECISIONS.md  

Project: **Senka**  
Type: Rust CLI HTTP Tooling Engine  
Status: Initial Architecture Lock  

---

## 1. Project Vision

**Senka** is a developer-native, **CLI-first** HTTP execution and inspection engine.

It provides:

- Structured HTTP request execution
- Environment + secret management
- Persistent structured logging
- Automation-friendly CLI workflows
- Optional terminal UI (TUI)

Senka is **local-first**: no telemetry, no cloud dependency, no background networking beyond user-invoked requests.

---

## 2. Core Philosophy

1. **Files are the source of truth.** (Requests + non-secret env values are stored as files.)
2. **Secrets never live in repository files.** (Keychain-only for secrets.)
3. **Logging is structured and queryable.** (SQLite, with retention.)
4. **Secure by default.** (Redaction on output + on storage.)
5. **Fast by design.** (Streaming, bounded buffering, minimal parsing.)
6. **Accessible by design.** (Readable output, keyboard-only workflows, no color-only meaning.)
7. **Explicit over magical behavior.** (Predictable resolution + no hidden discovery.)

---

## 3. Naming Decision

Project Name: **Senka**  
Meaning reference: 閃火 — “flash ignition”

Reasoning:

- Represents fast execution
- Anime-adjacent theme without franchise coupling
- Low legal risk for dev tooling
- CLI-friendly
- Scalable brand identity

Binary name: `senka`  
Workspace name: `senka`

---

## 4. Technical Stack

Language: Rust  
Async Runtime: Tokio  
HTTP Client: Reqwest  
Serialization: Serde  
YAML Parsing: serde_yaml  
CLI Parsing: Clap (derive)  
Database: SQLite (rusqlite)  
Secrets: keyring crate (OS keychain)  
TUI (optional): ratatui + crossterm  
IDs: ULID  

---

## 5. Workspace Architecture

Senka is structured as a Rust workspace.

```
senka/
  Cargo.toml (workspace)
  crates/
    core/
    runner/
    store/
    secrets/
    cli/
    tui/ (feature gated)
```

### Responsibilities

#### core

- Config models
- Request definitions
- Environment resolution
- Template rendering
- Redaction rules

#### runner

- HTTP execution
- Streaming response capture (bounded)
- Timing, retries (future)
- Error normalization

#### store

- SQLite initialization + migrations
- Log schema + retention pruning
- Query API + export (JSONL)

#### secrets

- OS keychain integration (get/set/delete)
- Secret lookup + caching policy (in-memory only)

#### cli

- Command parsing + orchestration
- Output formatting (human + JSON)
- Exit codes

#### tui (optional)

- Interactive request explorer
- Log viewer
- Env selector

---

## 6. Project Structure on Disk

```bash
project/
  tool.yml
  env/
    dev.yml
    stage.yml
  requests/
    users.get.yml
  data/
  .senka/
    logs.db
```

All logs live in `.senka/logs.db`.

---

## 7. Request Definition Format

YAML-based and editor-friendly.

Example:

```yaml
name: users.get
method: GET
url: "{{base_url}}/users/{{user_id}}"
headers:
  Accept: application/json
auth:
  type: bearer
  token: "{{token}}"
```

Supported bodies (v1):

- `raw` (string)
- `json` (object)
- `form` (k/v)

Templating syntax (v1):

```bash
{{variable_name}}
```

No logic / loops / conditionals in v1.

---

## 8. Environment Model

Plaintext environment files (non-sensitive):

- `env/dev.yml`, `env/stage.yml`, etc.

Secrets:

- Stored in OS keychain only
- Never written to disk
- Namespaced by `project_id + env_name + key`

Resolution order:

1. CLI overrides (`--var key=value`)
2. Plaintext env file
3. Secret store
4. Error if unresolved (unless explicitly optional in the future)

---

## 9. Logging Model

Database: SQLite

Tables:

**runs**

- id (ULID)
- ts (unix ms)
- project
- env
- request_name
- method
- url (redacted)
- status (nullable)
- duration_ms
- error (nullable)

**payloads**

- run_id (FK)
- request_headers (JSON)
- request_body (truncated, redacted)
- response_headers (JSON)
- response_body (truncated, redacted)

Indexes:

- `runs(ts)`
- `runs(request_name, ts)`
- `runs(status, ts)`

Retention:

- Default: 30 days (configurable)
- `senka log prune` enforces

---

## 10. Redaction Rules

Redaction applies:

1. Before terminal output
2. Before log storage

Defaults:

- Headers: `authorization`, `cookie`, `set-cookie`
- Any key marked secret by the secrets store
- Configurable: query params + JSON fields

Redaction is **ON by default**.
Override only via explicit flag (and still avoid writing unredacted secrets to storage).

---

## 11. CLI Command Model

**Project**

- `senka init`

**Environment**

- `senka env list`
- `senka env use <name>`
- `senka env set KEY=VALUE [--env <name>]`
- `senka env set-secret KEY [--env <name>]`
- `senka env export [--env <name>]` (redacted)

**Requests**

- `senka req list`
- `senka req new <name>`
- `senka run <request> [--env <name>] [--var key=value...]`
- `senka run <request> --copy curl`

**Logs**

- `senka log tail`
- `senka log list [filters...]`
- `senka log show <id>`
- `senka log prune [--keep 30d]`
- `senka log export --format jsonl`

**UI**

- `senka ui` (optional feature)

---

## 12. Security Decisions

- No secrets in plaintext files.
- Redaction is default and comprehensive.
- Bounded storage of bodies (size + retention).
- No telemetry, no hidden network calls.
- Safe defaults for TLS verification and redirects.

---

## 13. Non-Goals (v1)

- API design suite / OpenAPI editor
- OAuth device/interactive flows
- Desktop GUI
- Team sync
- Complex templating
- Assertion/test framework

These may be future expansions.

---

## 14. Stability Commitment

Stable surfaces (after 1.0):

- YAML request schema (additive changes only)
- CLI command names + flags (deprecated with warnings)
- Log schema (migration-based evolution)

Breaking changes require a major version bump.

---

## 15. Design Principles

- Deterministic behavior
- Minimal magic
- Transparent configuration
- Strong CLI ergonomics
- Human-readable project files
- Machine-friendly logs

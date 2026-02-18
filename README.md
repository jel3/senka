# senka

A CLI-first HTTP execution and inspection tool. Define requests as YAML files, run them with environment variables and secrets, and browse results in a terminal UI. Local-first — no telemetry, no cloud dependency.

## Installation

```bash
cargo build --release
# Binary at: target/release/senka
```

To build with the interactive TUI:

```bash
cargo build --release --features tui
```

## Quick Start

```bash
# Initialize a new project in the current directory
senka init

# Create a request
senka req new users.get

# Edit senka-requests/users.get.yml, then run it
senka run users.get --env dev

# Browse requests and logs interactively
senka tui  # requires --features tui
```

## Project Structure

After `senka init`:

```
my-project/
  senka.yml                  # Project config
  senka-env/
    dev.yml                  # Plaintext env variables
  senka-requests/
    example.get.yml          # Request definitions
  .senka/
    logs.db                  # SQLite log database (auto-created)
```

## senka.yml

```yaml
name: my-project

defaults:
  env: dev           # Default environment
  timeout_ms: 30000
  max_redirects: 10

redaction:
  headers:
    - authorization
    - cookie
  query_params:
    - token
    - api_key
  json_fields:
    - password
    - access_token

logging:
  enabled: true
  max_body_kb: 256
  retention_days: 30
```

## Request Files

Request files live in `senka-requests/` and are named `<name>.yml`.

```yaml
# senka-requests/users.get.yml
name: users.get
method: GET
url: "{{base_url}}/users"
headers:
  Authorization: "Bearer {{token}}"
query:
  page: "1"
```

Supported body types:

```yaml
# Raw body
body:
  raw: "hello world"

# JSON body
body:
  json:
    username: "{{user}}"
    password: "{{pass}}"

# Form body
body:
  form:
    grant_type: client_credentials
```

Auth shortcuts:

```yaml
# Bearer token
auth:
  type: bearer
  token: "{{token}}"

# Basic auth
auth:
  type: basic
  username: "{{user}}"
  password: "{{pass}}"
```

## Environment Files

Variables are defined per environment in `senka-env/<name>.yml`:

```yaml
# senka-env/dev.yml
base_url: http://localhost:3000
user: alice
```

Template syntax: `{{var_name}}` — applied to URL, headers, query params, and body.

Variable resolution order (highest wins):

1. `--var` CLI overrides
2. Environment file (`senka-env/<name>.yml`)
3. Secret store (OS keychain)

## Commands

### `senka init`

Initialize a new project in the current directory. Creates `senka.yml`, `senka-env/dev.yml`, and an example request.

### `senka run <request> [options]`

Execute a request.

```
Options:
  --env <name>        Environment to use (overrides default)
  --var KEY=VALUE     Variable override (repeatable)
  --show-headers      Print response headers
  --json              Output as JSON
  --fail              Exit with code 5 on non-2xx response
  --insecure          Disable TLS verification
  --no-redact         Skip redaction (shows secrets in output)
  --no-color          Disable color output
```

Examples:

```bash
senka run users.get --env dev
senka run users.create --env staging --var user=bob
senka run health --env prod --json
```

### `senka req list`

List all request files in the project.

### `senka req new <name>`

Create a new request file at `senka-requests/<name>.yml` with a starter template.

### `senka env list`

List all available environments.

### `senka env set-secret <key> [--env <name>]`

Store a secret in the OS keychain (prompted securely, never written to disk). The secret is available as `{{key}}` in request templates when the matching env is active.

```bash
senka env set-secret token --env dev
# Prompts: Enter secret value for 'token':
```

### `senka log tail`

Show the 20 most recent log entries.

### `senka log list [options]`

List log entries with optional filters.

```
Options:
  --since <duration>   e.g. 30m, 2h, 7d
  --status <code>      Filter by HTTP status
  --req <name>         Filter by request name (substring)
  --json               Output as JSON
```

### `senka log show <id>`

Show full detail for a log entry including request/response headers and body.

### `senka log prune [--keep <duration>]`

Delete log entries older than the given duration (default: `30d`).

### `senka log export`

Export all log entries to stdout as JSONL.

### `senka tui` *(requires `--features tui`)*

Launch the interactive terminal UI.

| Key | Action |
|-----|--------|
| `Tab` | Switch between Requests / Logs tabs |
| `↑` / `↓` or `j` / `k` | Navigate list |
| `Enter` | Run selected request (Requests tab) / Load detail (Logs tab) |
| `e` | Open environment selector |
| `Esc` | Clear response / close popup |
| `q` / `Ctrl+C` | Quit |

## Secrets

Secrets are stored in the OS keychain and **never written to any file**. They are redacted from logs and output by default.

```bash
# Store a secret
senka env set-secret api_key --env dev

# Use it in a request
url: "{{base_url}}/data?key={{api_key}}"
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Config / usage error |
| 3 | Network / TLS failure |
| 4 | Timeout |
| 5 | Non-2xx response (only with `--fail`) |

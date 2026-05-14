# Generate Senka Request Files from a REST API

Use this prompt in any project with a REST API to automatically generate Senka request YAML files.

---

## Prompt

```
Analyze this project's REST API endpoints and generate Senka request YAML files for each one.

## What to do

1. Find all REST API endpoints in this codebase (routes, controllers, handlers, etc.)
2. For each endpoint, create a YAML file in `senka-requests/` following the format below
3. Create a `senka.yml` project config
4. Create a `senka-env/dev.yml` with a `base_url` variable

## Request file format (`senka-requests/<name>.yml`)

Use the naming convention `<resource>.<method>.yml` (e.g., `users.get.yml`, `users.post.yml`, `orders-by-id.delete.yml`).

```yaml
name: <human-readable name>
method: GET | POST | PUT | PATCH | DELETE
url: "{{base_url}}/path/to/endpoint"

headers:
  Content-Type: application/json       # only if needed
  # add other headers the endpoint expects

query:                                  # optional, for query string params
  param_name: example_value

auth: null                              # or one of:
  # type: bearer
  # token: "{{api_token}}"
  #
  # type: basic
  # username: "{{username}}"
  # password: "{{password}}"

body: null                              # or one of:
  # type: json
  # value:
  #   field: value
  #
  # type: form
  # value:
  #   field: value
  #
  # type: raw
  # value: "raw string body"
```

## Rules

- Use `{{base_url}}` for the host in every URL — never hardcode it.
- Use `{{variable}}` template syntax for any value that would change per environment (tokens, IDs, keys).
- For endpoints that require authentication, use `auth.type: bearer` with `token: "{{api_token}}"` (or basic auth if the API uses that).
- For POST/PUT/PATCH endpoints, include a realistic example body matching the schema the endpoint expects.
- For path parameters like `/users/:id`, use a template variable: `{{base_url}}/users/{{user_id}}`.
- Set `headers`, `query`, `auth`, and `body` to `null` or omit them when not needed — don't leave empty objects.
- One file per endpoint. If an endpoint supports multiple methods, create separate files.

## Project config (`senka.yml`)

```yaml
name: <project-name>

defaults:
  env: dev
  timeout_ms: 30000
  max_redirects: 10

redaction:
  headers:
    - authorization
    - cookie
    - set-cookie
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

## Environment file (`senka-env/dev.yml`)

```yaml
base_url: http://localhost:<port>
# add any other variables referenced by {{}} in the request files
```

Put any secret values (API keys, passwords, tokens) as comments noting they should be set via `senka env set-secret`, not written to the env file.
```

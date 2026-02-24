# CLAUDE.md — algosearch

Rust CLI that discovers and searches Algolia-powered documentation sites from the terminal. Auto-extracts public search-only API credentials from doc site frontends. Designed for LLM agent integration.

## No Authentication Required

Uses **public search-only Algolia API credentials** embedded in documentation website frontends. The tool extracts these automatically via HTML/JS scanning and validates them against Algolia's API.

## Agent Mode

Auto-detects Claude Code (via `CLAUDE_CODE` env var or parent process name) and switches to JSON output. Can also be forced with `--agent` or `--json` flags.

## Global Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--json` | off (auto in agent mode) | Force JSON output |
| `-v, --verbose` | off | Show discovery progress |
| `--no-cache` | off | Skip registry, always discover fresh |
| `--timeout` | 10 | HTTP timeout in seconds |
| `--max-results` | 10 | Search hits per page |
| `--agent` | off (auto-detected) | Force agent mode |

## Commands

### add — Register a documentation site

```bash
algosearch add https://react.dev                                     # Auto-discover credentials
algosearch add https://react.dev --name react                        # Custom alias
algosearch add https://example.com --credentials APPID:APIKEY:INDEX  # Manual credentials
```

### search — Search registered sites

```bash
algosearch search react "hooks"                      # Search a registered site
algosearch search react "useState" --index react-v19 # Specific index
algosearch search --all "authentication"             # Search all registered sites
algosearch search react "hooks" --filter version:19  # Facet filter
```

| Flag | Description |
|------|-------------|
| `--all` | Search across all registered sites |
| `--index` | Target specific index (if site has multiple) |
| `--filter` | Facet filter as KEY:VALUE (repeatable) |
| `--page N` | Result page number (0-indexed, default: 0) |
| `--all-pages` | Fetch all pages of results |

### facets — List facet values for an attribute

```bash
algosearch facets myshop category                     # List all categories
algosearch facets myshop brand --max-values 500       # Top 500 brands
algosearch facets myshop category --index products_v2 # Specific index
```

| Flag | Description |
|------|-------------|
| `--max-values N` | Max facet values to return (default: 1000) |
| `--index` | Target specific index (if site has multiple) |

### export — Export all records from an index

```bash
# Export with partitioning (for large indexes >1000 records)
algosearch export myshop --partition-by category --format csv --output products.csv

# Export specific fields only
algosearch export myshop --partition-by category --fields sku,price,name,url

# Export as JSONL (default format)
algosearch export myshop --partition-by category --output products.jsonl

# Simple export without partitioning (max 1000 records)
algosearch export myshop --format csv
```

| Flag | Description |
|------|-------------|
| `--partition-by ATTR` | Facet attribute to partition by (recommended for large indexes) |
| `--format csv\|jsonl\|json` | Output format (default: jsonl) |
| `--output FILE` | Output file path (default: stdout) |
| `--fields f1,f2,...` | Comma-separated list of fields to export (default: all) |
| `--batch-size N` | Queries per multi-query API call (default: 5, max: 20) |
| `--index` | Target specific index (if site has multiple) |
| `--filter KEY:VALUE` | Filter to apply during export (repeatable) |

### query — One-shot discover + search (no save)

```bash
algosearch query https://react.dev "hooks"
algosearch query https://docs.stripe.com "webhooks" --filter version:2024
```

### ls — List registry

```bash
algosearch ls              # List all registered sites
algosearch ls react        # Show details for one site
```

### refresh — Re-discover credentials

```bash
algosearch refresh react   # Refresh one site
algosearch refresh --all   # Refresh all sites
```

### remove — Delete from registry

```bash
algosearch remove react
```

### discover — Discover credentials without saving

```bash
algosearch discover https://react.dev
```

### doctor — Run diagnostics

```bash
algosearch doctor    # Check config, registry, validate all site credentials
```

### completions — Shell completions

```bash
algosearch completions bash
algosearch completions zsh
algosearch completions fish
```

## Discovery Pipeline

Three-layer credential extraction:

1. **HTML scanning** — 9 strategies: `docsearch()` calls, `algoliasearch()` inits, meta tags, data attributes, framework hydration (`__NEXT_DATA__`, `__NUXT__`), escaped JSON (React Server Components), window globals, DNS preconnect hints, proximity patterns
2. **JS bundle scanning** — Fetches linked script files (up to 5, max 2MB each), scans for Algolia patterns and index names
3. **Validation** — Tests each candidate with a dummy query against `https://{appId}-dsn.algolia.net/1/indexes/{indexName}/query`

If discovery fails in agent mode, returns JSON with manual extraction steps.

## Schema Profiles

Auto-detects index schema and adapts search display:

| Profile | Detected By | Fields |
|---------|-------------|--------|
| **DocSearch** | `hierarchy.lvl0` field | 6-level hierarchy, content, url, anchor |
| **E-commerce** | `price*` field | title, description, url, price, image, brand, category |
| **Generic** | Fallback | title, description, url |

## Registry

- **Location**: `~/.config/algosearch/sites.json`
- **Format**: JSON with version, sites map (each with URL, indices, credentials, timestamps, field mappings)
- **Locking**: File-level advisory locks prevent concurrent writes
- **Atomic writes**: Temp file + rename pattern

## Build

```bash
cargo build --release              # Build to target/release/algosearch
make install                       # Install to ~/bin (or PREFIX=~/.local/bin)
cargo install --git https://github.com/nicolasacchi/algosearch   # From repo
```

## Project Structure

```
src/main.rs                        # Entry point, command dispatch
src/cli.rs                         # CLI argument parsing (clap)
src/agent.rs                       # LLM agent detection (4-layer)
src/context.rs                     # AppContext shared state
src/registry.rs                    # Registry management & persistence
src/search.rs                      # Algolia API search
src/schema.rs                      # Field mapping & schema detection
src/http.rs                        # HTTP client builder
src/error.rs                       # Error types
src/output.rs                      # Output formatting (human vs JSON)
src/display.rs                     # Text formatting for results
src/commands/add.rs                # add command
src/commands/search.rs             # search command
src/commands/query.rs              # query (one-shot) command
src/commands/list.rs               # ls command
src/commands/refresh.rs            # refresh command
src/commands/remove.rs             # remove command
src/commands/discover.rs           # discover-only command
src/commands/doctor.rs             # diagnostics command
src/commands/facets.rs             # facet value listing command
src/commands/export.rs             # full index export command
src/discovery/mod.rs               # Discovery orchestration
src/discovery/html.rs              # HTML scanning strategies
src/discovery/js.rs                # JS bundle scanning
src/discovery/extraction.rs        # JSON extraction utilities
src/discovery/patterns.rs          # Regex patterns for credentials
src/discovery/validate.rs          # Validation against Algolia API
src/discovery/llm_fallback.rs      # Fallback instructions for LLM agents
```

# algosearch

Discover and search [Algolia DocSearch](https://docsearch.algolia.com/) documentation sites from the terminal.

Most developer documentation sites embed public Algolia search-only API credentials in their frontend. `algosearch` autodiscovers these credentials, caches them in a local registry, and lets you search against them — from the terminal or from LLM coding agents.

## Installation

### From source

```bash
cargo install --path .
```

### Shell completions

```bash
# Bash
algosearch completions bash > ~/.local/share/bash-completion/completions/algosearch

# Zsh
algosearch completions zsh > ~/.zfunc/_algosearch

# Fish
algosearch completions fish > ~/.config/fish/completions/algosearch.fish
```

## Usage

### Add a documentation site

```bash
algosearch add https://react.dev
algosearch add https://docs.astro.build --name astro
```

Credentials are autodiscovered from the site's HTML and JavaScript bundles, validated against Algolia's API, and saved to a local registry.

### Search

```bash
algosearch search react "useState"
algosearch search react "hooks" --filter version:19
algosearch search --all "authentication"
```

### One-shot query (no registry)

Discover and search in one step without saving credentials:

```bash
algosearch query https://vuejs.org "composables"
```

### Discover credentials only

```bash
algosearch discover https://docusaurus.io
```

### Manage the registry

```bash
algosearch ls                  # List all registered sites
algosearch ls react            # Show details for a site
algosearch refresh react       # Re-discover credentials
algosearch refresh --all       # Refresh all sites
algosearch remove react        # Remove a site
```

### Manual credentials

If autodiscovery fails, add credentials manually:

```bash
algosearch add https://example.com --credentials APPID123AB:apikey123:index-name
```

### Diagnostics

```bash
algosearch doctor
```

## Agent mode

`algosearch` detects when it's being called by an LLM agent (Claude Code, Cursor, etc.) and automatically switches to JSON output. Detection methods:

- `--agent` flag
- Environment variables: `CLAUDE_CODE`, `CURSOR_SESSION`, `CODEX`, `CONTINUE_SESSION`, `AIDER_SESSION`, `LLM_AGENT=1`
- Parent process name detection
- Non-interactive stdin with no explicit `--json`

Force JSON output:

```bash
algosearch --json search react "hooks"
```

Example JSON output:

```json
{
  "site": "react",
  "query": "hooks",
  "total_hits": 42,
  "results": [
    {
      "title": "Built-in React Hooks",
      "hierarchy": ["React", "Reference", "Hooks"],
      "snippet": "Hooks let you use different React features...",
      "url": "https://react.dev/reference/react/hooks",
      "type": "lvl1"
    }
  ]
}
```

When autodiscovery fails in agent mode, the response includes `fallback_instructions` with steps the agent can follow to extract credentials via browser automation.

## Discovery layers

1. **HTML scanning** — DocSearch init calls, meta tags, data attributes, framework hydration blobs (`__NEXT_DATA__`, `__NUXT__`), generic config objects, escaped JSON (React Server Components), `window.env` globals, DNS preconnect hints, proximity-based pattern matching
2. **JS bundle scanning** — Fetches and scans JavaScript files linked from the page
3. **LLM fallback** — Returns structured instructions for agents to extract credentials via browser automation

## Configuration

Registry is stored at `~/.config/algosearch/sites.json` (Linux/macOS).

## License

MIT

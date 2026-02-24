# Valence CLI

Command-line interface for Valence v2 knowledge engine.

## Installation

```bash
go build -o valence-cli .
```

## Configuration

The CLI can be configured via:

1. **Config file** at `~/.config/valence/config.toml`
2. **Environment variables** prefixed with `VALENCE_`
3. **Command-line flags**

Priority: flags > env vars > config file

### Example config file

```toml
api-url = "http://localhost:8421"
token = "your-auth-token-here"
output = "table"
```

### Environment variables

```bash
export VALENCE_API_URL="http://localhost:8421"
export VALENCE_TOKEN="your-auth-token"
export VALENCE_OUTPUT="json"
```

## Usage

### Triple operations

```bash
# Insert a triple
valence triple insert --subject "Alice" --predicate "knows" --object "Bob"

# Query triples
valence triple query --subject "Alice"
valence triple query --predicate "knows"
valence triple query --subject "Alice" --predicate "knows"

# Get sources for a triple
valence triple get <triple-id>

# Search for similar nodes
valence triple search --query "Alice" --limit 5
```

### Node operations

```bash
# Get neighbors of a node (1-hop by default)
valence node neighbors Alice

# Get 2-hop neighborhood
valence node neighbors Alice --depth 2
```

### Maintenance operations

```bash
# Apply decay
valence maintenance decay --factor 0.95 --min-weight 0.01

# Evict low-weight triples
valence maintenance evict --threshold 0.1

# Recompute embeddings
valence maintenance recompute --dimensions 64
```

### Statistics and health

```bash
# Get engine statistics
valence stats

# Check API health
valence health
```

### Output formats

```bash
# Table output (default)
valence triple query --subject "Alice" --output table

# JSON output
valence triple query --subject "Alice" --output json

# Plain text output
valence triple query --subject "Alice" --output plain
```

## Commands not yet implemented

The following commands have stubs but the API endpoints don't exist yet:

- `valence session start/end/list/get`
- `valence federation status/peers`
- `valence trust check`

They will print "endpoint not yet available" when called.

## Development

### Building

```bash
go build -o valence-cli .
```

### Running tests

```bash
go test ./...
```

### Adding dependencies

```bash
go get github.com/some/package
go mod tidy
```

## Project structure

```
cli/
├── main.go                 # Entry point
├── go.mod                  # Module definition
├── cmd/                    # Command definitions
│   ├── root.go            # Root command + global flags
│   ├── triple.go          # Triple operations
│   ├── node.go            # Node operations
│   ├── maintenance.go     # Maintenance operations
│   ├── stats.go           # Statistics
│   ├── health.go          # Health check
│   ├── session.go         # Session stubs
│   ├── federation.go      # Federation stubs
│   ├── trust.go           # Trust stubs
│   └── version.go         # Version info
├── client/                 # HTTP client
│   ├── client.go          # Client implementation
│   └── types.go           # Request/response types
└── config/                 # Configuration
    └── config.go          # Config loading
```

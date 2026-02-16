# Valence Engine Deployment Guide

## Quick Start with Docker Compose

The easiest way to deploy Valence Engine is using Docker Compose:

```bash
# 1. Clone the repository
git clone <repository-url>
cd valence-v2

# 2. (Optional) Copy and customize environment variables
cp .env.example .env
# Edit .env to set your PostgreSQL password and other settings

# 3. Start services
docker-compose up

# 4. Verify deployment
curl http://localhost:8421/health
```

The engine will start with:
- **PostgreSQL** database for persistent storage
- **Valence Engine** HTTP server on port 8421
- Automatic health checks and graceful shutdown

## Configuration

### Environment Variables

Create a `.env` file or set these environment variables:

```bash
# PostgreSQL Configuration
DATABASE_URL=postgresql://valence:your_password@postgres:5432/valence
POSTGRES_PASSWORD=your_secure_password

# Server Configuration
VALENCE_HOST=0.0.0.0              # Bind address
VALENCE_PORT=8421                  # HTTP port
VALENCE_MODE=http                  # Server mode: http, mcp, or both

# Logging
RUST_LOG=info                      # Log level: trace, debug, info, warn, error
```

### Server Modes

- **http**: HTTP API server only (default, production mode)
- **mcp**: MCP (Model Context Protocol) server only (stdin/stdout)
- **both**: Run HTTP and MCP servers concurrently

## Health Monitoring

The `/health` endpoint provides comprehensive engine status:

```bash
curl http://localhost:8421/health | jq
```

Response includes:
- **Store type**: `postgres` or `memory`
- **Uptime**: Human-readable uptime
- **Storage stats**: Triple count, node count, utilization
- **Module status**: Embeddings, stigmergy, lifecycle, inference, resilience
- **Degradation level**: Current graceful degradation state

## Production Deployment

### Docker Build Features

The Dockerfile uses a multi-stage build with cargo-chef for optimal layer caching:
1. **Planner stage**: Generates dependency manifest
2. **Builder stage**: Builds dependencies (cached) and application
3. **Runtime stage**: Minimal Debian-slim image with only runtime dependencies

### Graceful Shutdown

The engine handles SIGTERM/SIGINT gracefully:
- In-flight requests are allowed to complete (30s grace period)
- PostgreSQL connections are closed cleanly
- State is flushed to disk

### Health Checks

Docker and docker-compose both include health checks:
- Health endpoint polled every 10 seconds
- 3 retries before marking unhealthy
- 15-second startup grace period

## Native Deployment (Without Docker)

For development or custom deployments:

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Install PostgreSQL (optional, for persistent storage)
# macOS: brew install postgresql
# Linux: apt install postgresql postgresql-contrib

# 3. Set up database
createdb valence
export DATABASE_URL=postgresql://user:pass@localhost:5432/valence

# 4. Build and run
cd engine
cargo run --release --features postgres -- --host 0.0.0.0 --port 8421

# Or use the run script
../scripts/run.sh --native
```

## Ports

- **8421**: Valence Engine HTTP API
- **5433**: PostgreSQL (mapped from internal 5432 to avoid conflicts)

## Volumes

Docker Compose creates a persistent volume for PostgreSQL:
- **valence-postgres-data**: Database storage

To backup/restore:

```bash
# Backup
docker exec valence-postgres pg_dump -U valence valence > backup.sql

# Restore
docker exec -i valence-postgres psql -U valence valence < backup.sql
```

## Startup Banner

On startup, the engine displays:
- Version information
- Configuration summary (mode, host, port, database)
- Module status (embeddings, stigmergy, lifecycle, inference)

Example:

```
╔══════════════════════════════════════════════════════════════════════════╗
║                          VALENCE ENGINE v0.1.0                          ║
║          Triple-based Knowledge Substrate with Topology Embeddings       ║
╚══════════════════════════════════════════════════════════════════════════╝
═══════════════════════════════════════════════════════════════════
Configuration Summary:
  Mode:         http
  Host:         0.0.0.0
  Port:         8421
  Database:     PostgreSQL (postgresql://valence:****@postgres:5432/valence)
  Log Level:    info
═══════════════════════════════════════════════════════════════════
Engine Status:
  Store Type:            postgres
  Triple Count:          0
  Node Count:            0
  Max Triples:           1000000
  Max Nodes:             100000
  Embeddings Enabled:    false
  Stigmergy Enabled:     true
  Lifecycle Management:  true
  Resilience Module:     true
  Inference Training:    true
═══════════════════════════════════════════════════════════════════
```

## API Documentation

See the full API documentation in `docs/api.md` or explore the endpoints:

- `GET /health` - Health check and status
- `POST /triples` - Insert triples
- `GET /triples` - Query triples
- `POST /search` - Semantic search
- `GET /stats` - Engine statistics
- `POST /maintenance/*` - Lifecycle operations

## Troubleshooting

### Container won't start
- Check logs: `docker-compose logs valence-engine`
- Verify PostgreSQL is healthy: `docker-compose ps`
- Check environment variables in `.env`

### Health check failing
- Ensure port 8421 is accessible
- Check logs for startup errors
- Verify PostgreSQL connection

### Performance issues
- Increase PostgreSQL resources in docker-compose.yml
- Tune RUST_LOG (reduce to `warn` or `error`)
- Check utilization via `/health` endpoint

### Database connection errors
- Verify DATABASE_URL format
- Ensure PostgreSQL container is running
- Check network connectivity: `docker-compose exec valence-engine ping postgres`

## Security Considerations

For production deployments:

1. **Change default passwords** in `.env`
2. **Use secrets management** (Docker secrets, Kubernetes secrets, etc.)
3. **Enable TLS** via reverse proxy (nginx, Caddy)
4. **Restrict network access** (firewall rules, security groups)
5. **Run as non-root** (already configured in Dockerfile)
6. **Keep dependencies updated** (`docker-compose pull && docker-compose up`)

## Monitoring

Recommended monitoring setup:

- **Health checks**: Poll `/health` endpoint
- **Logs**: Aggregate via Loki, Elasticsearch, or CloudWatch
- **Metrics**: Export via Prometheus (future enhancement)
- **Alerts**: Trigger on degradation level changes or health check failures

## Scaling

For horizontal scaling:
- Use a shared PostgreSQL instance
- Deploy multiple engine instances behind a load balancer
- Consider read replicas for PostgreSQL if read-heavy

For vertical scaling:
- Increase container CPU/memory limits
- Tune PostgreSQL `max_connections` and buffer sizes
- Adjust engine lifecycle bounds (`MAX_TRIPLES`, `MAX_NODES`)

# Valence v2 Deployment Guide

## Quick Start

### Option 1: Docker Compose (Recommended for Production)

The easiest way to deploy Valence with persistent PostgreSQL storage:

```bash
# Copy example environment file
cp .env.example .env

# Edit .env if needed (optional - defaults work for local testing)
nano .env

# Start services
docker-compose up -d

# Check status
docker-compose ps

# View logs
docker-compose logs -f valence-engine
```

The engine will be available at `http://localhost:8421`

### Option 2: Native (Development)

For local development without Docker:

```bash
# Option A: Memory-only storage (default)
./scripts/run.sh --native

# Option B: With PostgreSQL
# First, start a PostgreSQL instance (or use docker-compose for just postgres)
docker-compose up -d postgres

# Set DATABASE_URL and run
export DATABASE_URL="postgresql://valence:valence_dev_password@localhost:5433/valence"
./scripts/run.sh --native
```

### Option 3: Auto-detect

The run script will automatically choose the best option:

```bash
./scripts/run.sh
```

This will:
- Use `--native` if cargo is installed
- Fall back to `--docker` if only Docker is available

## Architecture

### Multi-stage Docker Build

The Dockerfile uses a two-stage build:

1. **Build stage**: Compiles the Rust application with PostgreSQL support
2. **Runtime stage**: Minimal Debian image with just the compiled binary

This keeps the final image small (~100MB) while maintaining full build capabilities.

### Services

**valence-engine**
- Valence triple store HTTP API
- Port: 8421
- Depends on: postgres (when using Docker Compose)
- Health check: `GET /health`

**postgres**
- PostgreSQL 16 with pgvector extension
- Port: 5433 (mapped to avoid conflicts with local PostgreSQL on 5432)
- Persistent volume: `valence-postgres-data`
- Health check: `pg_isready`

## Configuration

### Environment Variables

All configuration can be set via environment variables or `.env` file:

```bash
# Server Configuration
VALENCE_HOST=0.0.0.0              # Host to bind to
VALENCE_PORT=8421                  # Port to bind to

# Database Configuration
DATABASE_URL=postgresql://user:password@host:port/database

# PostgreSQL Password (used by docker-compose)
POSTGRES_PASSWORD=valence_dev_password

# Logging
RUST_LOG=info                      # trace, debug, info, warn, error
```

### Storage Backends

Valence supports two storage backends:

#### 1. Memory Store (Default)
- **Pros**: Fast, simple, no setup
- **Cons**: Data lost on restart, not suitable for production
- **Use case**: Development, testing, temporary workloads

```bash
# No DATABASE_URL needed
./scripts/run.sh --native
```

#### 2. PostgreSQL (Recommended)
- **Pros**: Persistent, reliable, scalable
- **Cons**: Requires PostgreSQL instance
- **Use case**: Production, persistent data

```bash
# Set DATABASE_URL
export DATABASE_URL="postgresql://valence:password@localhost:5433/valence"
./scripts/run.sh --native

# Or use docker-compose (includes PostgreSQL)
docker-compose up
```

## Deployment Scenarios

### Scenario 1: Local Development (Memory)

```bash
# Terminal 1: Run valence with memory storage
cd ~/projects/valence-v2
./scripts/run.sh --native

# Terminal 2: Test the API
curl http://localhost:8421/health
curl http://localhost:8421/stats
```

### Scenario 2: Local Development (PostgreSQL)

```bash
# Start just PostgreSQL
docker-compose up -d postgres

# Wait for postgres to be ready
docker-compose logs -f postgres  # Ctrl+C when ready

# Run valence natively with PostgreSQL
export DATABASE_URL="postgresql://valence:valence_dev_password@localhost:5433/valence"
./scripts/run.sh --native
```

### Scenario 3: Production (Docker Compose)

```bash
# On your server
git clone <repo-url> valence-v2
cd valence-v2

# Configure environment
cp .env.example .env
nano .env  # Set production passwords, etc.

# Start services in detached mode
docker-compose up -d

# Verify
docker-compose ps
curl http://localhost:8421/health

# View logs
docker-compose logs -f

# Stop
docker-compose down

# Stop and remove volumes (CAUTION: deletes data!)
docker-compose down -v
```

### Scenario 4: Production (Kubernetes)

For Kubernetes deployment, use the Docker image as a base:

```bash
# Build and push image
docker build -t myregistry/valence-engine:v0.1.0 .
docker push myregistry/valence-engine:v0.1.0

# Use in Kubernetes with external PostgreSQL
# (K8s manifests not included yet - TODO)
```

## API Endpoints

Once running, the following endpoints are available:

### Health & Stats
- `GET /health` - Health check
- `GET /stats` - Engine statistics

### Triples
- `POST /triples` - Insert triples
- `GET /triples?subject=...&predicate=...&object=...` - Query triples
- `GET /triples/{id}/sources` - Get triple sources

### Nodes
- `GET /nodes/{node}/neighbors?depth=N` - Get k-hop neighborhood

### Search
- `POST /search` - Semantic search using embeddings

### Maintenance
- `POST /maintenance/decay` - Apply decay to all triples
- `POST /maintenance/evict` - Remove low-weight triples
- `POST /maintenance/recompute-embeddings` - Recompute graph embeddings

## Troubleshooting

### "DATABASE_URL provided but postgres feature not enabled"

If you see this warning:
- Rebuild with `--features postgres`
- Or use docker-compose (already includes postgres feature)

```bash
cd engine
cargo build --release --features postgres
```

### Port 8421 already in use

Change the port in `.env`:

```bash
VALENCE_PORT=8422
```

Or pass it directly:

```bash
valence-engine --port 8422
```

### Docker Compose fails to start

Check logs for specific errors:

```bash
docker-compose logs valence-engine
docker-compose logs postgres
```

Common issues:
- Port conflicts: Change ports in docker-compose.yml
- Volume permissions: Check Docker volume mounts
- Database connection: Ensure postgres is healthy before starting valence-engine

### Database connection refused

If running natively with PostgreSQL:
1. Ensure PostgreSQL is running
2. Check DATABASE_URL format:
   ```
   postgresql://user:password@host:port/database
   ```
3. Verify port (5433 for docker-compose postgres, 5432 for system postgres)
4. Test connection:
   ```bash
   psql "$DATABASE_URL" -c "SELECT 1;"
   ```

## Maintenance

### Backup PostgreSQL Data

```bash
# Using docker-compose
docker-compose exec postgres pg_dump -U valence valence > backup.sql

# Restore
docker-compose exec -T postgres psql -U valence valence < backup.sql
```

### Upgrade

```bash
# Pull latest changes
git pull

# Rebuild and restart
docker-compose up -d --build
```

### View Logs

```bash
# All services
docker-compose logs -f

# Just valence-engine
docker-compose logs -f valence-engine

# Last 100 lines
docker-compose logs --tail=100 valence-engine
```

### Stop Services

```bash
# Stop (keeps data)
docker-compose stop

# Stop and remove containers (keeps volumes)
docker-compose down

# Stop and remove everything including data (DESTRUCTIVE!)
docker-compose down -v
```

## Performance Tuning

### PostgreSQL

For production, consider tuning PostgreSQL settings in `docker-compose.yml`:

```yaml
services:
  postgres:
    environment:
      POSTGRES_SHARED_BUFFERS: 256MB
      POSTGRES_EFFECTIVE_CACHE_SIZE: 1GB
      POSTGRES_WORK_MEM: 16MB
```

### Valence Engine

- Set `RUST_LOG=warn` or `RUST_LOG=error` in production to reduce logging overhead
- Consider running multiple instances behind a load balancer
- Use PostgreSQL connection pooling (already handled by sqlx)

## Security

For production deployments:

1. **Change default passwords**:
   ```bash
   POSTGRES_PASSWORD=<strong-random-password>
   ```

2. **Use environment-specific configs**:
   - Don't commit `.env` to version control
   - Use `.env.production`, `.env.staging`, etc.

3. **Enable TLS for PostgreSQL**:
   - Mount SSL certificates
   - Configure PostgreSQL for SSL connections

4. **Restrict network access**:
   - Use Docker networks
   - Firewall rules
   - VPN/private networking

5. **Regular backups**:
   - Automated PostgreSQL backups
   - Test restore procedures

## Development

### Build from Source

```bash
cd engine

# Without PostgreSQL
cargo build --release

# With PostgreSQL
cargo build --release --features postgres

# Run tests
cargo test

# With PostgreSQL tests
cargo test --features postgres
```

### Hot Reload Development

```bash
# Install cargo-watch
cargo install cargo-watch

# Auto-reload on changes
cd engine
cargo watch -x 'run --features postgres'
```

## Next Steps

- Set up monitoring (Prometheus, Grafana)
- Add health check integrations
- Configure reverse proxy (nginx, Caddy)
- Set up CI/CD pipeline
- Create Kubernetes manifests
- Add rate limiting
- Implement authentication/authorization

## Support

For issues, questions, or contributions:
- GitHub Issues: [Add your repo URL]
- Documentation: See `docs/` directory
- API Reference: See `docs/api.md`

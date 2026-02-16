#!/usr/bin/env bash
set -euo pipefail

# Valence Engine run script
# Usage: ./scripts/run.sh [--docker|--native]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

MODE="${1:-}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_command() {
    if ! command -v "$1" &> /dev/null; then
        return 1
    fi
    return 0
}

check_dependencies_native() {
    local missing=0
    
    if ! check_command cargo; then
        print_error "cargo not found. Install Rust from https://rustup.rs/"
        missing=1
    fi
    
    return $missing
}

check_dependencies_docker() {
    local missing=0
    
    if ! check_command docker; then
        print_error "docker not found. Install Docker from https://docs.docker.com/get-docker/"
        missing=1
    fi
    
    if ! check_command docker-compose && ! docker compose version &> /dev/null; then
        print_error "docker-compose not found. Install Docker Compose"
        missing=1
    fi
    
    return $missing
}

run_native() {
    print_info "Running Valence Engine in native mode..."
    
    if ! check_dependencies_native; then
        exit 1
    fi
    
    # Load .env if it exists
    if [ -f "$PROJECT_ROOT/.env" ]; then
        print_info "Loading environment from .env"
        set -a
        source "$PROJECT_ROOT/.env"
        set +a
    else
        print_warn "No .env file found. Using defaults (MemoryStore)"
    fi
    
    # Set default DATABASE_URL if not set
    if [ -z "${DATABASE_URL:-}" ]; then
        print_warn "DATABASE_URL not set. Using in-memory storage."
        print_info "To use PostgreSQL, set DATABASE_URL in .env or environment"
    fi
    
    cd "$PROJECT_ROOT/engine"
    
    # Build features based on DATABASE_URL
    if [ -n "${DATABASE_URL:-}" ]; then
        print_info "Building with PostgreSQL support..."
        cargo run --release --features postgres -- \
            --host "${VALENCE_HOST:-127.0.0.1}" \
            --port "${VALENCE_PORT:-8421}" \
            ${DATABASE_URL:+--database-url "$DATABASE_URL"}
    else
        print_info "Building without PostgreSQL support..."
        cargo run --release -- \
            --host "${VALENCE_HOST:-127.0.0.1}" \
            --port "${VALENCE_PORT:-8421}"
    fi
}

run_docker() {
    print_info "Running Valence Engine in Docker mode..."
    
    if ! check_dependencies_docker; then
        exit 1
    fi
    
    cd "$PROJECT_ROOT"
    
    # Check if .env exists, create from example if not
    if [ ! -f "$PROJECT_ROOT/.env" ] && [ -f "$PROJECT_ROOT/.env.example" ]; then
        print_warn ".env not found. Creating from .env.example"
        cp "$PROJECT_ROOT/.env.example" "$PROJECT_ROOT/.env"
        print_info "Please review .env and adjust settings if needed"
    fi
    
    # Determine docker compose command
    if docker compose version &> /dev/null; then
        DOCKER_COMPOSE="docker compose"
    else
        DOCKER_COMPOSE="docker-compose"
    fi
    
    print_info "Building and starting services..."
    $DOCKER_COMPOSE up --build
}

show_usage() {
    cat << EOF
Valence Engine Run Script

Usage: $0 [MODE]

Modes:
  --native    Run using cargo (local development)
  --docker    Run using Docker Compose (production-like)
  
If no mode is specified, defaults to --native if cargo is available,
otherwise attempts --docker.

Environment Variables:
  VALENCE_HOST      Host to bind to (default: 127.0.0.1 native, 0.0.0.0 docker)
  VALENCE_PORT      Port to bind to (default: 8421)
  DATABASE_URL      PostgreSQL connection string (optional, uses memory if not set)
  RUST_LOG          Log level (default: info)
  
Examples:
  $0 --native                    # Run locally with cargo
  $0 --docker                    # Run with Docker Compose
  DATABASE_URL=postgresql://... $0 --native  # Run with PostgreSQL
  
For more information, see docs/deployment.md
EOF
}

# Main logic
case "$MODE" in
    --native)
        run_native
        ;;
    --docker)
        run_docker
        ;;
    --help|-h)
        show_usage
        exit 0
        ;;
    "")
        # Auto-detect based on available tools
        if check_command cargo; then
            print_info "No mode specified. Defaulting to --native (cargo available)"
            run_native
        elif check_command docker; then
            print_info "No mode specified. Defaulting to --docker (cargo not found, docker available)"
            run_docker
        else
            print_error "Neither cargo nor docker found. Please install one of them."
            show_usage
            exit 1
        fi
        ;;
    *)
        print_error "Unknown mode: $MODE"
        show_usage
        exit 1
        ;;
esac

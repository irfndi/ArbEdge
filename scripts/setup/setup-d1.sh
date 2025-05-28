#!/usr/bin/env bash

# Setup script for Cloudflare D1 Database
set -euo pipefail

echo "🗄️ Setting up Cloudflare D1 Database for ArbEdge..."

# Check if pnpm is available
if ! command -v pnpm &> /dev/null; then
    echo "❌ Error: pnpm is required to run wrangler" >&2
    exit 1
fi

# Get script directory for relative paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR"

# Use existing D1 database
DB_NAME="prod-arb-edge"
DB_ID="879bf844-93b2-433d-9319-6e6065bbfdfd"

echo "📋 Using existing D1 database:"
echo "   Name: $DB_NAME"
echo "   ID: $DB_ID"

# Verify database exists - fail fast if not found
echo "🔍 Verifying database exists..."
if pnpm dlx wrangler d1 list | grep -q "$DB_NAME"; then
    echo "✅ Database '$DB_NAME' found"
else
    echo "❌ Error: Database '$DB_NAME' not found in Cloudflare account" >&2
    echo "💡 Please create the database first or check the database name" >&2
    exit 1
fi

# Initialize database schema if available using absolute path
SCHEMA_FILE="$PROJECT_ROOT/sql/schema.sql"
echo "🏗️ Initializing database schema..."
if [[ -f "$SCHEMA_FILE" ]]; then
    echo "📄 Using schema file: $SCHEMA_FILE"
    pnpm dlx wrangler d1 execute "$DB_NAME" --file="$SCHEMA_FILE"
    echo "✅ Database schema initialized"
else
    echo "⚠️  Schema file not found at: $SCHEMA_FILE"
    echo "⚠️  Skipping schema initialization"
fi

echo "✅ D1 Database setup completed!"
echo "📝 Database ID $DB_ID is configured in wrangler.toml"
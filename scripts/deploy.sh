#!/bin/bash
# Production Deployment Script for ArbEdge
# Deploys to Cloudflare Workers with all required services

set -e

echo "🚀 Starting ArbEdge Production Deployment..."

# Check if wrangler is installed via pnpm
if ! pnpm wrangler --version &> /dev/null; then
    echo "❌ Wrangler CLI not found. Installing via pnpm..."
    pnpm add -D wrangler@latest
fi

# Authenticate with Cloudflare (if not already authenticated)
echo "🔐 Checking Cloudflare authentication..."
if ! pnpm wrangler whoami &> /dev/null; then
    echo "Please authenticate with Cloudflare:"
    pnpm wrangler login
fi

# Set required secrets
echo "🔑 Setting up secrets..."

# Disable command echoing to prevent secrets from being logged
set +x

# Telegram Bot Token - check environment variable first
if [ -z "$TELEGRAM_BOT_TOKEN" ]; then
    echo "TELEGRAM_BOT_TOKEN not found in environment variables."
    read -s -p "Enter TELEGRAM_BOT_TOKEN: " TELEGRAM_BOT_TOKEN
    echo
else
    echo "✅ Using TELEGRAM_BOT_TOKEN from environment variables"
fi
pnpm wrangler secret put TELEGRAM_BOT_TOKEN --env production <<< "$TELEGRAM_BOT_TOKEN"

# Cloudflare API Token - check environment variable first
if [ -z "$CLOUDFLARE_API_TOKEN" ]; then
    echo "CLOUDFLARE_API_TOKEN not found in environment variables."
    read -s -p "Enter CLOUDFLARE_API_TOKEN: " CLOUDFLARE_API_TOKEN
    echo
else
    echo "✅ Using CLOUDFLARE_API_TOKEN from environment variables"
fi
pnpm wrangler secret put CLOUDFLARE_API_TOKEN --env production <<< "$CLOUDFLARE_API_TOKEN"

# Re-enable command echoing if it was previously enabled
set -x

# Create KV Namespaces
echo "📦 Creating KV namespaces..."
USER_PROFILES_ID=$(pnpm wrangler kv:namespace create "USER_PROFILES" --env production | grep -o 'id = "[^"]*"' | cut -d'"' -f2)
MARKET_CACHE_ID=$(pnpm wrangler kv:namespace create "PROD_BOT_MARKET_CACHE" --env production | grep -o 'id = "[^"]*"' | cut -d'"' -f2)
SESSION_STORE_ID=$(pnpm wrangler kv:namespace create "PROD_BOT_SESSION_STORE" --env production | grep -o 'id = "[^"]*"' | cut -d'"' -f2)

# Validate KV namespace IDs
if [ -z "$USER_PROFILES_ID" ] || [ "$USER_PROFILES_ID" = "null" ]; then
    echo "❌ Error: Failed to extract USER_PROFILES KV namespace ID"
    exit 1
fi

if [ -z "$MARKET_CACHE_ID" ] || [ "$MARKET_CACHE_ID" = "null" ]; then
    echo "❌ Error: Failed to extract MARKET_CACHE KV namespace ID"
    exit 1
fi

if [ -z "$SESSION_STORE_ID" ] || [ "$SESSION_STORE_ID" = "null" ]; then
    echo "❌ Error: Failed to extract SESSION_STORE KV namespace ID"
    exit 1
fi

echo "✅ KV Namespaces created:"
echo "  USER_PROFILES: $USER_PROFILES_ID"
echo "  PROD_BOT_MARKET_CACHE: $MARKET_CACHE_ID"
echo "  PROD_BOT_SESSION_STORE: $SESSION_STORE_ID"

# Create D1 Database
echo "🗄️ Creating D1 database..."
D1_DB_ID=$(pnpm wrangler d1 create arbitrage-production --env production | grep -o 'database_id = "[^"]*"' | cut -d'"' -f2)

# Validate D1 database ID
if [ -z "$D1_DB_ID" ] || [ "$D1_DB_ID" = "null" ]; then
    echo "❌ Error: Failed to extract D1 database ID"
    exit 1
fi

echo "✅ D1 Database created: $D1_DB_ID"

# Create R2 Buckets
echo "🪣 Creating R2 buckets..."
pnpm wrangler r2 bucket create arb-edge-market-data --env production
pnpm wrangler r2 bucket create arb-edge-analytics --env production
echo "✅ R2 Buckets created"

# Create Queues
echo "🚥 Creating Cloudflare Queues..."
pnpm wrangler queues create opportunity-distribution --env production
pnpm wrangler queues create user-notifications --env production
pnpm wrangler queues create analytics-events --env production
pnpm wrangler queues create dead-letter-queue --env production
echo "✅ Queues created"

# Create Pipelines
echo "🔄 Creating Cloudflare Pipelines..."
pnpm wrangler pipelines create market-data-pipeline --r2-bucket arb-edge-market-data --env production
pnpm wrangler pipelines create analytics-pipeline --r2-bucket arb-edge-analytics --env production
pnpm wrangler pipelines create audit-pipeline --r2-bucket arb-edge-analytics --env production
echo "✅ Pipelines created"

# Update wrangler.toml with actual IDs
echo "📝 Updating wrangler.toml with resource IDs..."

# Update USER_PROFILES ID
if ! sed -i.bak "s/your-kv-namespace-id-here/$USER_PROFILES_ID/g" wrangler.toml; then
    echo "❌ Error: Failed to update USER_PROFILES ID in wrangler.toml"
    exit 1
fi

# Update MARKET_CACHE ID
if ! sed -i.bak "s/your-market-cache-kv-id-here/$MARKET_CACHE_ID/g" wrangler.toml; then
    echo "❌ Error: Failed to update MARKET_CACHE ID in wrangler.toml"
    exit 1
fi

# Update SESSION_STORE ID
if ! sed -i.bak "s/your-session-kv-id-here/$SESSION_STORE_ID/g" wrangler.toml; then
    echo "❌ Error: Failed to update SESSION_STORE ID in wrangler.toml"
    exit 1
fi

# Update D1 database ID
if ! sed -i.bak "s/your-d1-database-id-here/$D1_DB_ID/g" wrangler.toml; then
    echo "❌ Error: Failed to update D1 database ID in wrangler.toml"
    exit 1
fi

# Clean up backup files after successful replacements
rm -f wrangler.toml.bak

echo "✅ wrangler.toml updated successfully"

# Run D1 migrations
echo "🔄 Running D1 migrations..."
pnpm wrangler d1 migrations apply arbitrage-production --env production

# Run CI pipeline before deployment
echo "🧪 Running CI pipeline to ensure code quality..."
make ci

# Build and deploy
echo "🔨 Building and deploying Worker..."
cargo install -q worker-build
worker-build --release

# Deploy to production
pnpm wrangler deploy --env production

echo "🎉 Deployment completed successfully!"
echo ""
echo "📋 Next steps:"
echo "1. Update your domain DNS to point to the Worker"
echo "2. Test all endpoints"
echo "3. Monitor logs: pnpm wrangler tail --env production"
echo "4. Check analytics in Cloudflare dashboard"
echo ""
echo "🔗 Useful commands:"
echo "  View logs: pnpm wrangler tail --env production"
echo "  Update secrets: pnpm wrangler secret put SECRET_NAME --env production"
echo "  Check KV data: pnpm wrangler kv:key list --binding USER_PROFILES --env production"
echo "  Query D1: pnpm wrangler d1 execute arbitrage-production --command 'SELECT * FROM users;' --env production"
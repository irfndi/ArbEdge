# ArbEdge Deployment Guide

## Environment Variables Setup

To avoid manually entering secrets during deployment, you can use environment variables:

### 1. Create Environment File

```bash
# Copy the example file
cp .env.example .env

# Edit with your actual values
vim .env  # or use your preferred editor
```

### 2. Required Environment Variables

- `TELEGRAM_BOT_TOKEN`: Get from @BotFather on Telegram
- `CLOUDFLARE_API_TOKEN`: Get from Cloudflare Dashboard > My Profile > API Tokens

### 3. Deploy with Environment Variables

```bash
# Source the environment file
source .env

# Run deployment (will use env vars automatically)
./scripts/deploy.sh
```

### 4. Alternative: Export Variables Directly

```bash
# Export variables in your shell
export TELEGRAM_BOT_TOKEN="your_token_here"
export CLOUDFLARE_API_TOKEN="your_token_here"

# Run deployment
./scripts/deploy.sh
```

## Security Notes

- ✅ `.env` files are already in `.gitignore`
- ✅ Never commit actual tokens to version control
- ✅ Use different tokens for development and production
- ✅ Rotate tokens regularly for security

## Deployment Process

The deployment script will:

1. Check for environment variables first
2. Prompt for manual input only if env vars are missing
3. Set up Cloudflare Workers secrets
4. Create KV namespaces and D1 databases
5. Run CI pipeline (`make ci`)
6. Build and deploy the application

## Troubleshooting

### "Token not found in environment variables"

This means the environment variable is not set. Either:
- Source your `.env` file: `source .env`
- Export the variable manually: `export TELEGRAM_BOT_TOKEN="your_token"`
- Enter it manually when prompted

### "Permission denied" on deploy.sh

Make the script executable:
```bash
chmod +x ./scripts/deploy.sh
```
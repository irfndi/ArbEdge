# ArbEdge Development Scripts

This directory contains automation scripts for development workflow and CI validation.

## 🚀 Quick Start

```bash
# Quick pre-commit validation
make quick                # or ./scripts/pre-commit.sh

# Full CI validation (recommended before push)
make validate            # or ./scripts/local-ci.sh

# Comprehensive quality analysis
make quality             # or ./scripts/full-check.sh
```

## 📋 Available Scripts

### `pre-commit.sh` - Quick Pre-commit Validation
**Purpose**: Fast validation before committing code  
**Time**: ~30-60 seconds  
**Use when**: Before every commit

**What it does**:
- ✅ Auto-formats code with `cargo fmt`
- ✅ Runs quick clippy lints
- ✅ Runs tests (skippable with `SKIP_TESTS=true`)
- ✅ Quick build check (skippable with `SKIP_BUILD=true`)
- ✅ Scans for TODO/FIXME and unwrap() patterns
- ✅ Shows staged files

**Usage**:
```bash
# Normal run
./scripts/pre-commit.sh

# Skip tests for faster run
SKIP_TESTS=true ./scripts/pre-commit.sh

# Skip both tests and build
SKIP_TESTS=true SKIP_BUILD=true ./scripts/pre-commit.sh

# Via Makefile
make pre-commit
make quick  # alias
```

### `local-ci.sh` - Full CI Pipeline Validation
**Purpose**: Run exact same checks as GitHub Actions CI  
**Time**: ~2-5 minutes  
**Use when**: Before pushing to remote, after major changes

**What it does**:
- ✅ Mirrors `.github/workflows/ci.yml` exactly
- ✅ Environment setup and WASM target verification
- ✅ Code formatting check (strict)
- ✅ Clippy linting (fail on warnings)
- ✅ Full test suite with verbose output
- ✅ WASM release build
- ✅ Wrangler deployment dry-run

**Usage**:
```bash
./scripts/local-ci.sh

# Via Makefile
make local-ci
make validate  # alias
make ci        # legacy alias
```

### `full-check.sh` - Comprehensive Quality Analysis
**Purpose**: Extensive code quality validation with coverage and metrics  
**Time**: ~5-15 minutes  
**Use when**: Before releases, weekly quality checks, investigating issues

**What it does**:
- ✅ Clean build from scratch
- ✅ Security audit (if cargo-audit installed)
- ✅ Comprehensive clippy (including pedantic rules)
- ✅ Full test suite with coverage report
- ✅ Both debug and release builds (native + WASM)
- ✅ Documentation generation test
- ✅ Wrangler deployment validation
- ✅ Code quality metrics and statistics
- ✅ Git status analysis

**Usage**:
```bash
./scripts/full-check.sh

# Via Makefile
make full-check
make quality  # alias
```

### `dev-setup.sh` - Development Environment Setup
**Purpose**: Initial setup and environment validation  
**Use when**: First time setup, troubleshooting environment issues

**What it does**:
- ✅ Verifies Rust toolchain configuration
- ✅ Installs WASM target if missing
- ✅ Validates WASM build capability
- ✅ Runs basic test validation

## 🔧 Environment Requirements

### Prerequisites

- **Rust**: v1.82+ (for Cloudflare Workers compilation)
- **Node.js**: v22.x (for wrangler)
- **PNPM**: Package manager for wrangler dependency
- **Cargo**: v1.82+ (comes with Rust)
- **Git**: Version control

### Required Tools

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install WASM target
rustup target add wasm32-unknown-unknown

# Install PNPM if not already installed
curl -fsSL https://get.pnpm.io/install.sh | sh -

# Install Wrangler via PNPM
pnpm add -D wrangler@latest
```

### Optional Tools (Enhanced Features)
```bash
# For security auditing
cargo install cargo-audit

# For test coverage
cargo install cargo-tarpaulin

# For code statistics
cargo install tokei
```

## 🎯 Recommended Workflow

### Daily Development
```bash
# 1. Make your changes
# 2. Quick validation before commit
make quick

# 3. Commit your changes
git add .
git commit -m "feat: your changes"

# 4. Full validation before push
make validate

# 5. Push to remote
git push
```

### Before Release
```bash
# Comprehensive quality check
make quality

# Review coverage report
open coverage/tarpaulin-report.html

# Review documentation
cargo doc --open
```

## 📊 Script Comparison

| Script | Time | Use Case | Strictness | Coverage |
|--------|------|----------|------------|----------|
| `pre-commit.sh` | ~1min | Before commits | Basic | No |
| `local-ci.sh` | ~3min | Before push | CI-level | No |
| `full-check.sh` | ~10min | Weekly/Release | Comprehensive | Yes |

## 🚨 Troubleshooting

### Script Fails: "Permission denied"
```bash
chmod +x scripts/*.sh
```

### Script Fails: "Command not found"
```bash
# Check if script exists
ls -la scripts/

# Ensure you're in project root
pwd
# Should show: /path/to/ArbEdge
```

### Tests Fail in Scripts
```bash
# Run tests directly to see detailed errors
cargo test --verbose

# Check for uncommitted changes
git status
```

### WASM Build Fails
```bash
# Verify WASM target
rustup target list --installed | grep wasm32

# Reinstall if missing
rustup target add wasm32-unknown-unknown
```

### Wrangler Issues
```bash
# Install/update wrangler
pnpm add -D wrangler@latest

# Check version
pnpm wrangler --version
```

## 💡 Tips

1. **Start small**: Use `make quick` for daily development
2. **CI confidence**: Always run `make validate` before pushing
3. **Release ready**: Use `make quality` before releases
4. **Parallel work**: Scripts are safe to run in parallel on different branches
5. **Environment variables**: Most scripts respect environment variables for customization 
#!/bin/bash
# Local CI script that mirrors GitHub Actions CI/CD pipeline
# Runs all the same checks as .github/workflows/ci.yml

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Utility functions
print_step() {
    echo -e "\n${BLUE}🔄 $1${NC}"
}

print_success() {
    echo -e "${GREEN}✅ $1${NC}"
}

print_warning() {
    echo -e "${YELLOW}⚠️  $1${NC}"
}

print_error() {
    echo -e "${RED}❌ $1${NC}"
}

print_header() {
    echo -e "\n${BLUE}=====================================${NC}"
    echo -e "${BLUE}🦀 ArbEdge Local CI Pipeline${NC}"
    echo -e "${BLUE}=====================================${NC}"
}

# Ensure we're using correct Rust toolchain
export PATH="$HOME/.cargo/bin:$PATH"

print_header

# Step 1: Environment setup (mirrors CI setup steps)
print_step "Setting up environment"
echo "Rust version: $(rustc --version)"
echo "Cargo version: $(cargo --version)"
echo "Node version: $(node --version 2>/dev/null || echo 'Node not found')"

# Step 2: Add WASM target (mirrors CI)
print_step "Adding WASM target"
if rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    print_success "WASM target already installed"
else
    rustup target add wasm32-unknown-unknown
    print_success "WASM target added"
fi

# Step 3: Check formatting (mirrors CI)
print_step "Checking code formatting"
if cargo fmt --all -- --check; then
    print_success "Code formatting is correct"
else
    print_error "Code formatting issues found. Run 'cargo fmt' to fix."
    exit 1
fi

# Step 4: Check compilation (mirrors CI)
print_step "Checking compilation for all targets"
if cargo check --all-targets --all-features; then
    print_success "Compilation check passed"
else
    print_error "Compilation errors found"
    exit 1
fi

# Step 5: Run linter (mirrors CI)
print_step "Running clippy linter"
if cargo clippy --all-targets --all-features -- -D warnings; then
    print_success "Clippy checks passed"
else
    print_error "Clippy warnings/errors found"
    exit 1
fi

# Step 6: Run tests (mirrors CI)
print_step "Running tests"
if cargo test --verbose --all-targets --all-features; then
    print_success "All tests passed"
else
    print_error "Tests failed"
    exit 1
fi

# Step 7: Build for WASM (mirrors CI)
print_step "Building for WASM target"
if cargo build --target wasm32-unknown-unknown --release; then
    print_success "WASM build successful"
else
    print_error "WASM build failed"
    exit 1
fi

# Step 8: Test wrangler build (mirrors CI dry-run)
print_step "Testing wrangler build (dry-run)"
if pnpm wrangler --version >/dev/null 2>&1; then
    echo "Wrangler version: $(pnpm wrangler --version)"
    if pnpm wrangler deploy --dry-run; then
        print_success "Wrangler dry-run successful"
    else
        print_error "Wrangler dry-run failed"
        exit 1
    fi
else
    print_warning "Wrangler not installed, skipping dry-run test"
    print_warning "Install with: pnpm add -D wrangler@latest"
fi

# Final summary
echo -e "\n${GREEN}=====================================${NC}"
echo -e "${GREEN}🎉 All CI checks passed locally!${NC}"
echo -e "${GREEN}=====================================${NC}"
echo -e "Your code is ready for commit and push."
echo -e "The GitHub Actions CI should pass without issues." 
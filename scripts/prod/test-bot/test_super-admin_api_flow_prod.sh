#!/bin/bash

# ArbEdge Production API Test Script
# Tests super-admin functionality with real D1 database data
# Validates core system functionality for production deployment

set -euo pipefail

# Configuration
BASE_URL="${BASE_URL:-https://arb-edge.irfandimarsya.workers.dev}"
TEST_OUTPUT_DIR="./test_results"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_FILE="$TEST_OUTPUT_DIR/prod_api_test_$TIMESTAMP.log"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

# Test counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Pre-flight checks for required external commands
echo "🔍 Checking required external commands..."
REQUIRED_COMMANDS=("wrangler" "jq" "curl")
for cmd in "${REQUIRED_COMMANDS[@]}"; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "❌ Error: Required command '$cmd' is not installed or not in PATH"
        echo "💡 Please install $cmd and ensure it's available in your PATH"
        exit 1
    fi
    echo "✅ Found: $cmd"
done

# Create test output directory
mkdir -p "$TEST_OUTPUT_DIR"

# Logging function
log() {
    echo -e "$1" | tee -a "$LOG_FILE"
}

# Test function - accepts curl arguments as array to avoid eval
run_test() {
    local test_name="$1"
    local expected_status="$2"
    local validation_function="$3"
    shift 3
    local curl_args=("$@")
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    log "${BLUE}[TEST $TOTAL_TESTS] $test_name${NC}"
    
    # Execute curl command and capture response
    local response_file="$TEST_OUTPUT_DIR/response_$TOTAL_TESTS.json"
    local status_code
    
    status_code=$(curl "${curl_args[@]}" \
        --max-time 30 \
        -w "%{http_code}" \
        -s \
        -o "$response_file" \
        2>/dev/null || echo "000")
    
    # Check status code
    if [ "$status_code" = "$expected_status" ]; then
        log "${GREEN}✅ Status Code: $status_code (Expected: $expected_status)${NC}"
        
        # Run validation function if provided
        if [ -n "$validation_function" ] && command -v "$validation_function" >/dev/null; then
            if $validation_function "$response_file"; then
                log "${GREEN}✅ Response Validation: PASSED${NC}"
                PASSED_TESTS=$((PASSED_TESTS + 1))
            else
                log "${RED}❌ Response Validation: FAILED${NC}"
                FAILED_TESTS=$((FAILED_TESTS + 1))
            fi
        else
            PASSED_TESTS=$((PASSED_TESTS + 1))
        fi
    else
        log "${RED}❌ Status Code: $status_code (Expected: $expected_status)${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
    fi
    
    # Show response preview
    if [ -f "$response_file" ]; then
        log "${YELLOW}Response Preview:${NC}"
        head -c 300 "$response_file" | tee -a "$LOG_FILE"
        echo "" | tee -a "$LOG_FILE"
    fi
    
    log "----------------------------------------"
}

# Validation functions
validate_health_response() {
    local response_file="$1"
    jq -e '.success == true and .data.status == "healthy"' "$response_file" >/dev/null
}

validate_error_response() {
    local response_file="$1"
    jq -e '.success == false and .error != null' "$response_file" >/dev/null
}

validate_admin_profile() {
    local response_file="$1"
    jq -e '.success == true and .data.user_id != null and (.data.subscription_tier == "admin" or .data.subscription_tier == "super_admin")' "$response_file" >/dev/null
}

validate_opportunities_list() {
    local response_file="$1"
    jq -e '.success == true and (.data | type) == "array" and (.data | length) >= 0' "$response_file" >/dev/null
}

validate_analytics_data() {
    local response_file="$1"
    jq -e '.success == true and .data != null' "$response_file" >/dev/null
}

validate_d1_data() {
    local response_file="$1"
    jq -e '.success == true and .data != null and (.data | type) == "array"' "$response_file" >/dev/null
}

validate_user_management() {
    local response_file="$1"
    jq -e '.success == true and .data.users != null' "$response_file" >/dev/null
}

# Fetch real Super Admin credentials from production D1 database
echo "🔍 Fetching real admin data from production D1 database..."
SUPER_ADMIN_USER_ID=$(wrangler d1 execute prod-arb-edge --command="SELECT user_id FROM user_profiles WHERE user_id LIKE '%superadmin%' LIMIT 1" --json | jq -r '.[0].results[0].user_id')
SUPER_ADMIN_TELEGRAM_ID=$(wrangler d1 execute prod-arb-edge --command="SELECT telegram_id FROM user_profiles WHERE user_id LIKE '%superadmin%' LIMIT 1" --json | jq -r '.[0].results[0].telegram_id')

if [ "$SUPER_ADMIN_USER_ID" = "null" ] || [ -z "$SUPER_ADMIN_USER_ID" ]; then
    echo "❌ Error: No admin user found in production D1 database"
    echo "💡 Please ensure an admin user exists in the user_profiles table"
    exit 1
fi

if [ "$SUPER_ADMIN_TELEGRAM_ID" = "null" ] || [ -z "$SUPER_ADMIN_TELEGRAM_ID" ]; then
    echo "❌ Error: No admin telegram ID found in production D1 database"
    echo "💡 Please ensure the admin user has a valid telegram_id in the user_profiles table"
    exit 1
fi

echo "✅ Found admin user: $SUPER_ADMIN_USER_ID (Telegram ID: $SUPER_ADMIN_TELEGRAM_ID)"

# Start testing
log "${PURPLE}🚀 Starting ArbEdge Production API Tests (Super Admin Only)${NC}"
log "${PURPLE}Base URL: $BASE_URL${NC}"
log "${PURPLE}Super Admin User ID: $SUPER_ADMIN_USER_ID${NC}"
log "${PURPLE}Timestamp: $TIMESTAMP${NC}"
log "========================================"

# 1. Health Check Tests
log "${YELLOW}📋 HEALTH CHECK TESTS${NC}"

run_test "Basic Health Check" "200" \
    "validate_health_response" \
    -X GET "$BASE_URL/api/v1/health"

run_test "Detailed Health Check" "200" \
    "validate_health_response" \
    -X GET "$BASE_URL/api/v1/health/detailed"

# 2. Super Admin Authentication Tests
log "${YELLOW}🔐 SUPER ADMIN AUTHENTICATION TESTS${NC}"

# Test with super admin credentials
run_test "Super Admin Profile Access" "200" \
    "validate_admin_profile" \
    -X GET "$BASE_URL/api/v1/users/profile" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Super Admin Preferences" "200" \
    "" \
    -X GET "$BASE_URL/api/v1/users/preferences" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 3. D1 Database Integration Tests
log "${YELLOW}🗄️ D1 DATABASE INTEGRATION TESTS${NC}"

# Test D1 database queries through API
run_test "Get All Users from D1" "200" \
    "validate_d1_data" \
    -X GET "$BASE_URL/api/v1/admin/users" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Get User Sessions from D1" "200" \
    "validate_d1_data" \
    -X GET "$BASE_URL/api/v1/admin/sessions" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Get Opportunities from D1" "200" \
    "validate_d1_data" \
    -X GET "$BASE_URL/api/v1/admin/opportunities" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Get User Profiles from D1" "200" \
    "validate_d1_data" \
    -X GET "$BASE_URL/api/v1/admin/user-profiles" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 4. Super Admin Opportunity Access Tests
log "${YELLOW}💰 SUPER ADMIN OPPORTUNITY ACCESS TESTS${NC}"

# Test super admin opportunity access (should have unlimited access)
run_test "List Opportunities - Super Admin" "200" \
    "validate_opportunities_list" \
    -X GET "$BASE_URL/api/v1/opportunities?limit=100" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Premium Features Access - Super Admin" "200" \
    "validate_opportunities_list" \
    -X GET "$BASE_URL/api/v1/opportunities?premium=true" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Advanced Opportunities - Super Admin" "200" \
    "validate_opportunities_list" \
    -X GET "$BASE_URL/api/v1/opportunities?advanced=true&ai_enhanced=true" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 5. Super Admin Analytics Access Tests
log "${YELLOW}📊 SUPER ADMIN ANALYTICS ACCESS TESTS${NC}"

# Super admin should have access to all analytics
run_test "Dashboard Analytics - Super Admin" "200" \
    "validate_analytics_data" \
    -X GET "$BASE_URL/api/v1/analytics/dashboard" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "System Analytics - Super Admin" "200" \
    "validate_analytics_data" \
    -X GET "$BASE_URL/api/v1/analytics/system" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "User Analytics - Super Admin" "200" \
    "validate_analytics_data" \
    -X GET "$BASE_URL/api/v1/analytics/users" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Performance Analytics - Super Admin" "200" \
    "validate_analytics_data" \
    -X GET "$BASE_URL/api/v1/analytics/performance" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 6. Super Admin Management Functions
log "${YELLOW}👑 SUPER ADMIN MANAGEMENT TESTS${NC}"

# Test super admin management capabilities
run_test "User Management Access" "200" \
    "validate_user_management" \
    -X GET "$BASE_URL/api/v1/admin/manage/users" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "System Configuration Access" "200" \
    "" \
    -X GET "$BASE_URL/api/v1/admin/config/system" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Invitation Code Management" "200" \
    "" \
    -X GET "$BASE_URL/api/v1/admin/invitations" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 7. Real Exchange API Integration Tests
log "${YELLOW}🏦 EXCHANGE API INTEGRATION TESTS${NC}"

# Test real exchange API calls through super admin
run_test "Exchange Balance - Super Admin" "200" \
    "" \
    -X GET "$BASE_URL/api/v1/trading/balance" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Exchange Markets - Super Admin" "200" \
    "" \
    -X GET "$BASE_URL/api/v1/trading/markets" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Exchange Opportunities - Super Admin" "200" \
    "" \
    -X GET "$BASE_URL/api/v1/trading/opportunities" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 8. AI Intelligence Service Tests
log "${YELLOW}🤖 AI INTELLIGENCE SERVICE TESTS${NC}"

# Test AI services with super admin access
run_test "AI Market Analysis - Super Admin" "200" \
    "" \
    -X POST "$BASE_URL/api/v1/ai/analyze" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"pair": "BTC/USDT", "exchanges": ["binance", "bybit"]}'

run_test "AI Risk Assessment - Super Admin" "200" \
    "" \
    -X POST "$BASE_URL/api/v1/ai/risk-assessment" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"portfolio": {"BTC": 1.0, "ETH": 5.0}}'

# Helper function to generate Telegram payload
generate_telegram_payload() {
    local command="$1"
    jq -n \
        --arg telegram_id "$SUPER_ADMIN_TELEGRAM_ID" \
        --arg command "$command" \
        '{
            "message": {
                "chat": {"id": ($telegram_id | tonumber)},
                "from": {"id": ($telegram_id | tonumber), "username": "superadmin"},
                "text": $command
            }
        }'
}

# 9. Telegram Bot Integration Tests
log "${YELLOW}🤖 TELEGRAM BOT INTEGRATION TESTS${NC}"

# Test Telegram webhook with super admin - using helper function for payload generation
TELEGRAM_START_PAYLOAD=$(generate_telegram_payload "/start")
TELEGRAM_OPPORTUNITIES_PAYLOAD=$(generate_telegram_payload "/opportunities")
TELEGRAM_ADMIN_STATS_PAYLOAD=$(generate_telegram_payload "/admin_stats")

run_test "Telegram Start Command - Super Admin" "200" \
    "" \
    -X POST "$BASE_URL/webhook/telegram" \
    -H "Content-Type: application/json" \
    -d "$TELEGRAM_START_PAYLOAD"

run_test "Telegram Opportunities Command - Super Admin" "200" \
    "" \
    -X POST "$BASE_URL/webhook/telegram" \
    -H "Content-Type: application/json" \
    -d "$TELEGRAM_OPPORTUNITIES_PAYLOAD"

run_test "Telegram Admin Stats Command - Super Admin" "200" \
    "" \
    -X POST "$BASE_URL/webhook/telegram" \
    -H "Content-Type: application/json" \
    -d "$TELEGRAM_ADMIN_STATS_PAYLOAD"

# 10. Performance and Load Tests
log "${YELLOW}⚡ PERFORMANCE TESTS${NC}"

# Test system performance with super admin access
log "${BLUE}Testing concurrent requests with super admin${NC}"
start_time=$(date +%s)

for _ in {1..20}; do
    curl -X GET "$BASE_URL/api/v1/opportunities" \
        -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
        -s -o /dev/null &
done

wait
end_time=$(date +%s)
duration=$((end_time - start_time))

log "${GREEN}✅ 20 concurrent requests completed in ${duration}s${NC}"

# 11. Error Handling Tests
log "${YELLOW}🚨 ERROR HANDLING TESTS${NC}"

# Test error scenarios even with super admin
run_test "Invalid JSON Payload" "400" \
    "validate_error_response" \
    -X POST "$BASE_URL/api/v1/opportunities/execute" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d "invalid json"

run_test "Non-existent Endpoint" "404" \
    "validate_error_response" \
    -X GET "$BASE_URL/api/v1/nonexistent" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# Test Summary
log "========================================"
log "${PURPLE}📊 PRODUCTION TEST SUMMARY${NC}"
log "${GREEN}✅ Passed: $PASSED_TESTS${NC}"
log "${RED}❌ Failed: $FAILED_TESTS${NC}"
log "${BLUE}📋 Total: $TOTAL_TESTS${NC}"

if [ $FAILED_TESTS -eq 0 ]; then
    log "${GREEN}🎉 ALL PRODUCTION TESTS PASSED! System ready for public beta.${NC}"
    log "${GREEN}✅ Super Admin functionality validated with real D1 data${NC}"
    log "${GREEN}✅ Core system functionality confirmed working${NC}"
    log "${GREEN}✅ If super admin passes, other user tiers should work correctly${NC}"
    exit 0
else
    log "${RED}💥 Some production tests failed. Check the logs for details.${NC}"
    log "${YELLOW}⚠️ Review failed tests before proceeding with deployment${NC}"
    exit 1
fi
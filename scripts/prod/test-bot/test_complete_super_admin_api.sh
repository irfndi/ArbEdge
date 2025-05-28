#!/bin/bash

# ArbEdge Complete Super Admin API Test Script
# Tests ALL functionality with super admin access
# Comprehensive coverage of every endpoint and feature

set -euo pipefail

# Configuration
BASE_URL="${BASE_URL:-https://arb-edge.irfandimarsya.workers.dev}"
TEST_OUTPUT_DIR="@logs"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_FILE="$TEST_OUTPUT_DIR/complete_super_admin_api_test_$TIMESTAMP.log"
SUPER_ADMIN_USER_ID="superadmin_1082762347"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Test counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Create test output directory
mkdir -p "$TEST_OUTPUT_DIR"

# Logging function
log() {
    echo -e "$1" | tee -a "$LOG_FILE"
}

# Test function
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
validate_success_response() {
    local response_file="$1"
    jq -e '.success == true and .data != null' "$response_file" >/dev/null
}

validate_health_response() {
    local response_file="$1"
    jq -e '.success == true and .data.status == "healthy"' "$response_file" >/dev/null
}

validate_array_response() {
    local response_file="$1"
    jq -e '.success == true and (.data | type) == "array"' "$response_file" >/dev/null
}

validate_user_profile() {
    local response_file="$1"
    jq -e '.success == true and .data.user_id != null and .data.subscription_tier != null' "$response_file" >/dev/null
}

validate_error_response() {
    local response_file="$1"
    jq -e '.success == false and .error != null' "$response_file" >/dev/null
}

# Start testing
log "${PURPLE}🚀 Starting ArbEdge Complete Super Admin API Tests${NC}"
log "${PURPLE}Base URL: $BASE_URL${NC}"
log "${PURPLE}Super Admin User ID: $SUPER_ADMIN_USER_ID${NC}"
log "${PURPLE}Timestamp: $TIMESTAMP${NC}"
log "========================================"

# 1. HEALTH CHECK ENDPOINTS
log "${CYAN}🏥 HEALTH CHECK ENDPOINTS${NC}"

run_test "Basic Health Check" "200" \
    "" \
    -X GET "$BASE_URL/health"

run_test "API v1 Health Check" "200" \
    "validate_health_response" \
    -X GET "$BASE_URL/api/v1/health"

run_test "API v1 Detailed Health Check" "200" \
    "validate_health_response" \
    -X GET "$BASE_URL/api/v1/health/detailed"

# 2. KV TEST ENDPOINT
log "${CYAN}🗄️ KV STORAGE TEST${NC}"

run_test "KV Test Endpoint" "200" \
    "" \
    -X GET "$BASE_URL/kv-test?value=super_admin_test"

# 3. USER MANAGEMENT ENDPOINTS (Super Admin Access)
log "${CYAN}👤 USER MANAGEMENT ENDPOINTS${NC}"

run_test "Get Super Admin Profile" "200" \
    "validate_user_profile" \
    -X GET "$BASE_URL/api/v1/users/profile" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Update Super Admin Profile" "200" \
    "validate_success_response" \
    -X PUT "$BASE_URL/api/v1/users/profile" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"risk_tolerance": 0.8, "preferred_pairs": ["BTC/USDT", "ETH/USDT", "SOL/USDT"]}'

run_test "Get Super Admin Preferences" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/users/preferences" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Update Super Admin Preferences" "200" \
    "validate_success_response" \
    -X PUT "$BASE_URL/api/v1/users/preferences" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"risk_tolerance": 0.9, "auto_trading_enabled": true, "notification_settings": {"opportunities": true, "price_alerts": true}}'

# 4. OPPORTUNITY ENDPOINTS (All Access)
log "${CYAN}💰 OPPORTUNITY ENDPOINTS${NC}"

run_test "Get Basic Opportunities" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/opportunities" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Get Premium Opportunities" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/opportunities?premium=true" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Execute Opportunity" "200" \
    "validate_success_response" \
    -X POST "$BASE_URL/api/v1/opportunities/execute" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"opportunity_id": "super_admin_test_opp", "amount": 1000}'

run_test "Find Opportunities (Legacy)" "200" \
    "" \
    -X POST "$BASE_URL/find-opportunities" \
    -H "Content-Type: application/json" \
    -d '{"exchanges": ["binance", "bybit"], "pairs": ["BTC/USDT"], "min_profit": 0.1}'

# 5. ANALYTICS ENDPOINTS (Full Access)
log "${CYAN}📊 ANALYTICS ENDPOINTS${NC}"

run_test "Dashboard Analytics" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/analytics/dashboard" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "System Analytics" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/analytics/system" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "User Analytics" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/analytics/users" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Performance Analytics" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/analytics/performance" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "User-Specific Analytics" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/analytics/user" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 6. ADMIN ENDPOINTS (Full Access)
log "${CYAN}👑 ADMIN ENDPOINTS${NC}"

run_test "Admin - Get All Users" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/admin/users" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Admin - Get User Sessions" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/admin/sessions" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Admin - Get Opportunities" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/admin/opportunities" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Admin - Get User Profiles" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/admin/user-profiles" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Admin - User Management" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/admin/manage/users" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Admin - System Configuration" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/admin/config/system" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Admin - Invitation Management" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/admin/invitations" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 7. TRADING ENDPOINTS (Full Access)
log "${CYAN}💹 TRADING ENDPOINTS${NC}"

run_test "Trading - Get Balance" "200" \
    "validate_success_response" \
    -X GET "$BASE_URL/api/v1/trading/balance" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Trading - Get Markets" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/trading/markets" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

run_test "Trading - Get Opportunities" "200" \
    "validate_array_response" \
    -X GET "$BASE_URL/api/v1/trading/opportunities" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID"

# 8. AI ENDPOINTS (Full Access)
log "${CYAN}🤖 AI ENDPOINTS${NC}"

run_test "AI - Market Analysis" "200" \
    "validate_success_response" \
    -X POST "$BASE_URL/api/v1/ai/analyze" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"pair": "BTC/USDT", "exchanges": ["binance", "bybit"], "timeframe": "1h"}'

run_test "AI - Risk Assessment" "200" \
    "validate_success_response" \
    -X POST "$BASE_URL/api/v1/ai/risk-assessment" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"portfolio": {"BTC": 0.5, "ETH": 2.0, "USDT": 5000}, "risk_tolerance": 0.8}'

# 9. EXCHANGE ENDPOINTS
log "${CYAN}🏦 EXCHANGE ENDPOINTS${NC}"

run_test "Exchange - Get Markets" "200" \
    "" \
    -X GET "$BASE_URL/exchange/markets"

run_test "Exchange - Get Ticker" "200" \
    "" \
    -X GET "$BASE_URL/exchange/ticker?exchange=binance&symbol=BTCUSDT"

run_test "Exchange - Get Funding Rate" "200" \
    "" \
    -X GET "$BASE_URL/exchange/funding?exchange=binance&symbol=BTCUSDT"

run_test "Exchange - Get Order Book" "200" \
    "" \
    -X GET "$BASE_URL/exchange/orderbook?exchange=binance&symbol=BTCUSDT"

# 10. POSITION MANAGEMENT ENDPOINTS
log "${CYAN}📈 POSITION MANAGEMENT ENDPOINTS${NC}"

# Create a position first
run_test "Positions - Create Position" "200" \
    "" \
    -X POST "$BASE_URL/positions" \
    -H "Content-Type: application/json" \
    -d '{
        "user_id": "'"$SUPER_ADMIN_USER_ID"'",
        "exchange": "binance",
        "pair": "BTC/USDT",
        "side": "long",
        "size_usd": 1000.0,
        "entry_price": 45000.0,
        "risk_percentage": 0.02
    }'

run_test "Positions - Get All Positions" "200" \
    "" \
    -X GET "$BASE_URL/positions"

# Note: Individual position operations would need a real position ID

# 11. TELEGRAM WEBHOOK TEST
log "${CYAN}📱 TELEGRAM WEBHOOK TEST${NC}"

run_test "Telegram Webhook - Test Message" "200" \
    "" \
    -X POST "$BASE_URL/webhook" \
    -H "Content-Type: application/json" \
    -d '{
        "update_id": 123456789,
        "message": {
            "message_id": 1,
            "from": {
                "id": 1082762347,
                "is_bot": false,
                "first_name": "Super",
                "last_name": "Admin"
            },
            "chat": {
                "id": 1082762347,
                "first_name": "Super",
                "last_name": "Admin",
                "type": "private"
            },
            "date": 1640995200,
            "text": "/status"
        }
    }'

# 12. ERROR HANDLING TESTS
log "${CYAN}🚨 ERROR HANDLING TESTS${NC}"

run_test "Invalid JSON Payload" "400" \
    "validate_error_response" \
    -X POST "$BASE_URL/api/v1/opportunities/execute" \
    -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
    -H "Content-Type: application/json" \
    -d '{"invalid": json}'

run_test "Non-existent Endpoint" "404" \
    "" \
    -X GET "$BASE_URL/api/v1/non-existent-endpoint"

run_test "Missing Authentication" "401" \
    "validate_error_response" \
    -X GET "$BASE_URL/api/v1/users/profile"

# 13. PERFORMANCE TESTS
log "${CYAN}⚡ PERFORMANCE TESTS${NC}"

log "${YELLOW}Testing concurrent requests...${NC}"
start_time=$(date +%s)
for i in {1..10}; do
    curl -X GET "$BASE_URL/api/v1/health" \
        -H "X-User-ID: $SUPER_ADMIN_USER_ID" \
        -s -o /dev/null &
done
wait
end_time=$(date +%s)
duration=$((end_time - start_time))
log "${GREEN}✅ 10 concurrent requests completed in ${duration}s${NC}"

# Generate comprehensive summary
log "========================================"
log "${PURPLE}📋 COMPLETE SUPER ADMIN API TEST SUMMARY${NC}"
log "${GREEN}✅ Passed: $PASSED_TESTS${NC}"
log "${RED}❌ Failed: $FAILED_TESTS${NC}"
log "${BLUE}📋 Total: $TOTAL_TESTS${NC}"

if [ $FAILED_TESTS -eq 0 ]; then
    log "${GREEN}🎉 ALL TESTS PASSED! Super Admin has full access to all functionality.${NC}"
    log "${GREEN}✅ Complete API coverage validated${NC}"
    log "${GREEN}✅ All endpoints responding correctly${NC}"
    log "${GREEN}✅ Super Admin permissions working perfectly${NC}"
else
    log "${RED}❌ Some tests failed. Review the logs above for details.${NC}"
    log "${YELLOW}💡 Failed tests may indicate:${NC}"
    log "${YELLOW}   - Service configuration issues${NC}"
    log "${YELLOW}   - Database connectivity problems${NC}"
    log "${YELLOW}   - External API dependencies${NC}"
    log "${YELLOW}   - Infrastructure limitations${NC}"
fi

log "📁 Detailed logs saved to: $LOG_FILE"
log "📊 Individual responses saved to: $TEST_OUTPUT_DIR/response_*.json"

# Exit with appropriate code
if [ $FAILED_TESTS -eq 0 ]; then
    exit 0
else
    exit 1
fi
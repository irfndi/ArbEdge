#!/bin/bash

# ArbEdge API v1 Comprehensive Test Script
# Tests all direct API endpoints with RBAC validation
# No Telegram webhook required - direct API access

set -euo pipefail

# Configuration
BASE_URL="${BASE_URL:-http://localhost:8787}"
TEST_OUTPUT_DIR="./test_results"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
LOG_FILE="$TEST_OUTPUT_DIR/api_v1_test_$TIMESTAMP.log"

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

validate_error_response() {
    local response_file="$1"
    jq -e '.success == false and .error != null' "$response_file" >/dev/null
}

validate_health_response() {
    local response_file="$1"
    jq -e '.success == true and .data.status == "healthy"' "$response_file" >/dev/null
}

validate_user_profile() {
    local response_file="$1"
    jq -e '.success == true and .data.user_id != null and .data.subscription_tier != null' "$response_file" >/dev/null
}

validate_opportunities_list() {
    local response_file="$1"
    jq -e '.success == true and (.data | type) == "array"' "$response_file" >/dev/null
}

validate_analytics_data() {
    local response_file="$1"
    jq -e '.success == true and .data != null' "$response_file" >/dev/null
}

# Test users for different subscription tiers
FREE_USER="user_free_123"
BASIC_USER="user_basic_234"
PREMIUM_USER="user_premium_456"
ENTERPRISE_USER="user_enterprise_678"
PRO_USER="user_pro_789"  # Backward compatibility - maps to enterprise
ADMIN_USER="user_admin_000"

# Start testing
log "${PURPLE}🚀 Starting ArbEdge API v1 Comprehensive Tests${NC}"
log "${PURPLE}Base URL: $BASE_URL${NC}"
log "${PURPLE}Timestamp: $TIMESTAMP${NC}"
log "========================================"

# 1. Health Check Tests (No authentication required)
log "${YELLOW}📋 HEALTH CHECK TESTS${NC}"

run_test "Basic Health Check" "200" \
    "validate_health_response" \
    -X GET "$BASE_URL/api/v1/health"

run_test "Detailed Health Check" "200" \
    "validate_health_response" \
    -X GET "$BASE_URL/api/v1/health/detailed"

# 2. Authentication Tests
log "${YELLOW}🔐 AUTHENTICATION TESTS${NC}"

# Test without authentication headers
run_test "No Authentication - User Profile" "401" \
    "validate_error_response" \
    -X GET "$BASE_URL/api/v1/users/profile"

run_test "No Authentication - Opportunities" "401" \
    "validate_error_response" \
    -X GET "$BASE_URL/api/v1/opportunities"

# 3. User Profile Tests (All Tiers)
log "${YELLOW}👤 USER PROFILE TESTS${NC}"

for tier_info in "free_user:$FREE_USER" "basic_user:$BASIC_USER" "premium_user:$PREMIUM_USER" "enterprise_user:$ENTERPRISE_USER" "pro_user:$PRO_USER" "admin_user:$ADMIN_USER"; do
    tier=$(echo "$tier_info" | cut -d: -f1)
    user_id=$(echo "$tier_info" | cut -d: -f2)
    
    log "${BLUE}Testing $tier ($user_id)${NC}"
    
    # Get user profile
    run_test "Get Profile - $tier" "200" \
        "validate_user_profile" \
        -X GET "$BASE_URL/api/v1/users/profile" \
        -H "X-User-ID: $user_id"
    
    # Get user preferences
    run_test "Get Preferences - $tier" "200" \
        "validate_success_response" \
        -X GET "$BASE_URL/api/v1/users/preferences" \
        -H "X-User-ID: $user_id"
    
    # Update preferences
    run_test "Update Preferences - $tier" "200" \
        "validate_success_response" \
        -X PUT "$BASE_URL/api/v1/users/preferences" \
        -H "X-User-ID: $user_id" \
        -H "Content-Type: application/json" \
        -d '{"risk_tolerance": 0.7, "preferred_pairs": ["BTC/USDT", "ETH/USDT"]}'
done

# 4. Opportunity Access Tests (RBAC Validation)
log "${YELLOW}💰 OPPORTUNITY ACCESS TESTS${NC}"

for tier_info in "free_user:$FREE_USER" "basic_user:$BASIC_USER" "premium_user:$PREMIUM_USER" "enterprise_user:$ENTERPRISE_USER" "pro_user:$PRO_USER" "admin_user:$ADMIN_USER"; do
    tier=$(echo "$tier_info" | cut -d: -f1)
    user_id=$(echo "$tier_info" | cut -d: -f2)
    
    log "${BLUE}Testing opportunity access for $tier ($user_id)${NC}"
    
    # Basic opportunities (all users should have access)
    run_test "Basic Opportunities - $tier" "200" \
        "validate_opportunities_list" \
        -X GET "$BASE_URL/api/v1/opportunities" \
        -H "X-User-ID: $user_id"
    
    # Premium opportunities (free users should be blocked)
    if [[ "$tier" == "free_user" ]]; then
        run_test "Premium Opportunities - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X GET "$BASE_URL/api/v1/opportunities?premium=true" \
            -H "X-User-ID: $user_id"
    else
        run_test "Premium Opportunities - $tier" "200" \
            "validate_opportunities_list" \
            -X GET "$BASE_URL/api/v1/opportunities?premium=true" \
            -H "X-User-ID: $user_id"
    fi
    
    # Execute opportunity
    run_test "Execute Opportunity - $tier" "200" \
        "validate_success_response" \
        -X POST "$BASE_URL/api/v1/opportunities/execute" \
        -H "X-User-ID: $user_id" \
        -H "Content-Type: application/json" \
        -d '{"opportunity_id": "test_opp_123", "amount": 100}'
done

# 5. Analytics Access Tests (Pro/Admin only)
log "${YELLOW}📊 ANALYTICS ACCESS TESTS${NC}"

for tier_info in "free_user:$FREE_USER" "premium_user:$PREMIUM_USER" "pro_user:$PRO_USER" "admin_user:$ADMIN_USER"; do
    tier=$(echo "$tier_info" | cut -d: -f1)
    user_id=$(echo "$tier_info" | cut -d: -f2)
    
    log "${BLUE}Testing analytics access for $tier ($user_id)${NC}"
    
    # User-specific analytics (all users should have access)
    run_test "User Analytics - $tier" "200" \
        "validate_analytics_data" \
        -X GET "$BASE_URL/api/v1/analytics/user" \
        -H "X-User-ID: $user_id"
    
    # Dashboard analytics (Pro/Admin only)
    if [[ "$tier" == "pro_user" || "$tier" == "admin_user" ]]; then
        run_test "Dashboard Analytics - $tier" "200" \
            "validate_analytics_data" \
            -X GET "$BASE_URL/api/v1/analytics/dashboard" \
            -H "X-User-ID: $user_id"
        
        run_test "User Management Analytics - $tier" "200" \
            "validate_analytics_data" \
            -X GET "$BASE_URL/api/v1/analytics/users" \
            -H "X-User-ID: $user_id"
        
        run_test "Performance Analytics - $tier" "200" \
            "validate_analytics_data" \
            -X GET "$BASE_URL/api/v1/analytics/performance" \
            -H "X-User-ID: $user_id"
    else
        run_test "Dashboard Analytics - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X GET "$BASE_URL/api/v1/analytics/dashboard" \
            -H "X-User-ID: $user_id"
    fi
    
    # System analytics (Admin only)
    if [[ "$tier" == "admin_user" ]]; then
        run_test "System Analytics - $tier" "200" \
            "validate_analytics_data" \
            -X GET "$BASE_URL/api/v1/analytics/system" \
            -H "X-User-ID: $user_id"
    else
        run_test "System Analytics - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X GET "$BASE_URL/api/v1/analytics/system" \
            -H "X-User-ID: $user_id"
    fi
done

# 6. Admin Endpoints Tests (Admin only)
log "${YELLOW}👑 ADMIN ENDPOINTS TESTS${NC}"

for tier_info in "free_user:$FREE_USER" "premium_user:$PREMIUM_USER" "pro_user:$PRO_USER" "admin_user:$ADMIN_USER"; do
    tier=$(echo "$tier_info" | cut -d: -f1)
    user_id=$(echo "$tier_info" | cut -d: -f2)
    
    log "${BLUE}Testing admin access for $tier ($user_id)${NC}"
    
    if [[ "$tier" == "admin_user" ]]; then
        # Admin should have access to all admin endpoints
        run_test "Admin Users - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/users" \
            -H "X-User-ID: $user_id"
        
        run_test "Admin Sessions - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/sessions" \
            -H "X-User-ID: $user_id"
        
        run_test "Admin Opportunities - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/opportunities" \
            -H "X-User-ID: $user_id"
        
        run_test "Admin User Profiles - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/user-profiles" \
            -H "X-User-ID: $user_id"
        
        run_test "Admin User Management - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/manage/users" \
            -H "X-User-ID: $user_id"
        
        run_test "Admin System Config - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/config/system" \
            -H "X-User-ID: $user_id"
        
        run_test "Admin Invitations - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/admin/invitations" \
            -H "X-User-ID: $user_id"
    else
        # Non-admin users should be blocked
        run_test "Admin Users - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X GET "$BASE_URL/api/v1/admin/users" \
            -H "X-User-ID: $user_id"
    fi
done

# 7. Trading Endpoints Tests (Premium+ only)
log "${YELLOW}🏦 TRADING ENDPOINTS TESTS${NC}"

for tier_info in "free_user:$FREE_USER" "premium_user:$PREMIUM_USER" "pro_user:$PRO_USER" "admin_user:$ADMIN_USER"; do
    tier=$(echo "$tier_info" | cut -d: -f1)
    user_id=$(echo "$tier_info" | cut -d: -f2)
    
    log "${BLUE}Testing trading access for $tier ($user_id)${NC}"
    
    if [[ "$tier" == "free_user" ]]; then
        # Free users should be blocked from trading endpoints
        run_test "Trading Balance - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X GET "$BASE_URL/api/v1/trading/balance" \
            -H "X-User-ID: $user_id"
        
        run_test "Trading Markets - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X GET "$BASE_URL/api/v1/trading/markets" \
            -H "X-User-ID: $user_id"
    else
        # Premium+ users should have access
        run_test "Trading Balance - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/trading/balance" \
            -H "X-User-ID: $user_id"
        
        run_test "Trading Markets - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/trading/markets" \
            -H "X-User-ID: $user_id"
        
        run_test "Trading Opportunities - $tier" "200" \
            "validate_success_response" \
            -X GET "$BASE_URL/api/v1/trading/opportunities" \
            -H "X-User-ID: $user_id"
    fi
done

# 8. AI Endpoints Tests (Premium+ only)
log "${YELLOW}🤖 AI ENDPOINTS TESTS${NC}"

for tier_info in "free_user:$FREE_USER" "premium_user:$PREMIUM_USER" "pro_user:$PRO_USER" "admin_user:$ADMIN_USER"; do
    tier=$(echo "$tier_info" | cut -d: -f1)
    user_id=$(echo "$tier_info" | cut -d: -f2)
    
    log "${BLUE}Testing AI access for $tier ($user_id)${NC}"
    
    if [[ "$tier" == "free_user" ]]; then
        # Free users should be blocked from AI endpoints
        run_test "AI Analysis - $tier (Should Fail)" "403" \
            "validate_error_response" \
            -X POST "$BASE_URL/api/v1/ai/analyze" \
            -H "X-User-ID: $user_id" \
            -H "Content-Type: application/json" \
            -d '{"pair": "BTC/USDT", "exchanges": ["binance", "bybit"]}'
    else
        # Premium+ users should have access
        run_test "AI Analysis - $tier" "200" \
            "validate_success_response" \
            -X POST "$BASE_URL/api/v1/ai/analyze" \
            -H "X-User-ID: $user_id" \
            -H "Content-Type: application/json" \
            -d '{"pair": "BTC/USDT", "exchanges": ["binance", "bybit"]}'
        
        run_test "AI Risk Assessment - $tier" "200" \
            "validate_success_response" \
            -X POST "$BASE_URL/api/v1/ai/risk-assessment" \
            -H "X-User-ID: $user_id" \
            -H "Content-Type: application/json" \
            -d '{"portfolio": {"BTC": 1.0, "ETH": 5.0}}'
    fi
done

# 9. Error Handling Tests
log "${YELLOW}🚨 ERROR HANDLING TESTS${NC}"

# Test invalid JSON
run_test "Invalid JSON Payload" "400" \
    "" \
    -X POST "$BASE_URL/api/v1/opportunities/execute" \
    -H "X-User-ID: $ADMIN_USER" \
    -H "Content-Type: application/json" \
    -d "invalid json"

# Test non-existent endpoint
run_test "Non-existent Endpoint" "404" \
    "" \
    -X GET "$BASE_URL/api/v1/nonexistent" \
    -H "X-User-ID: $ADMIN_USER"

# 10. Performance Tests
log "${YELLOW}⚡ PERFORMANCE TESTS${NC}"

log "${BLUE}Testing concurrent requests${NC}"
start_time=$(date +%s)

for _ in {1..10}; do
    curl -X GET "$BASE_URL/api/v1/health" \
        -s -o /dev/null &
done

wait
end_time=$(date +%s)
duration=$((end_time - start_time))

log "${GREEN}✅ 10 concurrent requests completed in ${duration}s${NC}"

# Test Summary
log "========================================"
log "${PURPLE}📊 API v1 TEST SUMMARY${NC}"
log "${GREEN}✅ Passed: $PASSED_TESTS${NC}"
log "${RED}❌ Failed: $FAILED_TESTS${NC}"
log "${BLUE}📋 Total: $TOTAL_TESTS${NC}"

if [ $FAILED_TESTS -eq 0 ]; then
    log "${GREEN}🎉 ALL API v1 TESTS PASSED! Direct API access working perfectly.${NC}"
    log "${GREEN}✅ RBAC validation working correctly${NC}"
    log "${GREEN}✅ All subscription tiers properly enforced${NC}"
    log "${GREEN}✅ Authentication and authorization working${NC}"
    exit 0
else
    log "${RED}💥 Some API v1 tests failed. Check the logs for details.${NC}"
    log "${YELLOW}⚠️ Review failed tests before proceeding${NC}"
    exit 1
fi 
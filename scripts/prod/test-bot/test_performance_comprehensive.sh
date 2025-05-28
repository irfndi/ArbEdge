#!/opt/homebrew/bin/bash

# ArbEdge Comprehensive Performance Testing Script
# Tests API performance, service injection overhead, and system limits

set -e

# Ensure we're using bash 4.0+ (required for associative arrays)
if [[ ${BASH_VERSINFO[0]} -lt 4 ]]; then
    echo "This script requires bash 4.0 or later. Current version: $BASH_VERSION"
    echo "Please install a newer version of bash: brew install bash"
    exit 1
fi

# Configuration
BASE_URL="${BASE_URL:-http://localhost:8787}"
CONCURRENT_USERS="${CONCURRENT_USERS:-50}"
REQUESTS_PER_USER="${REQUESTS_PER_USER:-10}"
STRESS_DURATION="${STRESS_DURATION:-30}"
WARMUP_REQUESTS="${WARMUP_REQUESTS:-20}"

# Test users for different subscription tiers
FREE_USER="user_free_123"
BASIC_USER="user_basic_234"
PREMIUM_USER="user_premium_456"
ENTERPRISE_USER="user_enterprise_678"
ADMIN_USER="user_admin_000"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Performance metrics
declare -A response_times
declare -A success_counts
declare -A error_counts
total_requests=0
total_errors=0
total_time=0

log() {
    echo -e "$1"
}

# Function to measure response time
measure_response_time() {
    local url="$1"
    local method="${2:-GET}"
    local headers="$3"
    local data="$4"
    
    local start_time=$(date +%s%N)
    
    if [[ "$method" == "POST" ]]; then
        if [[ -n "$headers" ]]; then
            local response=$(eval "curl -X POST \"$url\" $headers -H \"Content-Type: application/json\" -d \"$data\" -s -w \"%{http_code}\" -o /dev/null 2>/dev/null")
        else
            local response=$(curl -X POST "$url" \
                -H "Content-Type: application/json" \
                -d "$data" \
                -s -w "%{http_code}" -o /dev/null 2>/dev/null)
        fi
    else
        if [[ -n "$headers" ]]; then
            local response=$(eval "curl -X GET \"$url\" $headers -s -w \"%{http_code}\" -o /dev/null 2>/dev/null")
        else
            local response=$(curl -X GET "$url" \
                -s -w "%{http_code}" -o /dev/null 2>/dev/null)
        fi
    fi
    
    local end_time=$(date +%s%N)
    local duration=$(( (end_time - start_time) / 1000000 )) # Convert to milliseconds
    
    echo "$response:$duration"
}

# Function to run concurrent requests
run_concurrent_test() {
    local test_name="$1"
    local url="$2"
    local concurrent_users="$3"
    local requests_per_user="$4"
    local method="${5:-GET}"
    local headers="$6"
    local data="$7"
    
    log "${CYAN}🚀 Running $test_name${NC}"
    log "${BLUE}   Concurrent Users: $concurrent_users${NC}"
    log "${BLUE}   Requests per User: $requests_per_user${NC}"
    log "${BLUE}   Total Requests: $((concurrent_users * requests_per_user))${NC}"
    
    temp_dir=$(mktemp -d)
    local start_time=$(date +%s%N)
    
    # Launch concurrent users
    for ((user=1; user<=concurrent_users; user++)); do
        {
            set +e  # Disable exit on error for background processes
            user_success=0
            user_errors=0
            user_total_time=0
            
            for ((req=1; req<=requests_per_user; req++)); do
                result=$(measure_response_time "$url" "$method" "$headers" "$data")
                status_code=$(echo "$result" | cut -d: -f1)
                response_time=$(echo "$result" | cut -d: -f2)
                
                if [[ "$status_code" =~ ^[2-3][0-9][0-9]$ ]]; then
                    ((user_success++))
                else
                    ((user_errors++))
                fi
                
                user_total_time=$((user_total_time + response_time))
            done
            
            echo "$user_success:$user_errors:$user_total_time" > "$temp_dir/user_$user.result"
            set -e  # Re-enable exit on error
        } &
    done
    
    # Wait for all users to complete
    wait
    
    local end_time=$(date +%s%N)
    local total_duration=$(( (end_time - start_time) / 1000000 )) # Convert to milliseconds
    
    # Aggregate results
    local total_success=0
    local total_errors=0
    local total_response_time=0
    local min_response_time=999999
    local max_response_time=0
    
    for ((user=1; user<=concurrent_users; user++)); do
        if [[ -f "$temp_dir/user_$user.result" ]]; then
            result=$(cat "$temp_dir/user_$user.result")
            user_success=$(echo "$result" | cut -d: -f1)
            user_errors=$(echo "$result" | cut -d: -f2)
            user_time=$(echo "$result" | cut -d: -f3)
            
            total_success=$((total_success + user_success))
            total_errors=$((total_errors + user_errors))
            total_response_time=$((total_response_time + user_time))
            
            if [[ $user_time -lt $min_response_time ]]; then
                min_response_time=$user_time
            fi
            if [[ $user_time -gt $max_response_time ]]; then
                max_response_time=$user_time
            fi
        fi
    done
    
    # Calculate metrics
    local total_requests=$((total_success + total_errors))
    local success_rate=$(( total_requests > 0 ? (total_success * 100) / total_requests : 0 ))
    local avg_response_time=$(( total_requests > 0 ? total_response_time / total_requests : 0 ))
    local throughput=$(( total_duration > 0 ? (total_requests * 1000) / total_duration : 0 ))
    
    # Display results
    log "${GREEN}✅ $test_name Results:${NC}"
    log "   📊 Total Requests: $total_requests"
    log "   ✅ Successful: $total_success ($success_rate%)"
    log "   ❌ Failed: $total_errors"
    log "   ⏱️  Average Response Time: ${avg_response_time}ms"
    log "   🚀 Throughput: ${throughput} req/sec"
    log "   ⏰ Total Duration: ${total_duration}ms"
    log ""
    
    # Cleanup
    rm -rf "$temp_dir"
    
    # Store results for summary
    response_times["$test_name"]=$avg_response_time
    success_counts["$test_name"]=$total_success
    error_counts["$test_name"]=$total_errors
}

# Function to test service injection performance
test_service_injection_performance() {
    log "${YELLOW}🔧 SERVICE INJECTION PERFORMANCE TESTS${NC}"
    log "========================================"
    
    # Test webhook endpoint (service injection heavy)
    local webhook_payload='{
        "update_id": 123456789,
        "message": {
            "message_id": 1,
            "from": {"id": 123456789, "is_bot": false, "first_name": "Test"},
            "chat": {"id": 123456789, "type": "private"},
            "date": 1640995200,
            "text": "/status"
        }
    }'
    
    run_concurrent_test "Webhook Service Injection" \
        "$BASE_URL/webhook" \
        10 \
        5 \
        "POST" \
        "" \
        "$webhook_payload"
    
    # Test API v1 endpoints (lighter service usage)
    run_concurrent_test "API v1 Health Check" \
        "$BASE_URL/api/v1/health" \
        20 \
        10 \
        "GET"
    
    run_concurrent_test "API v1 User Profile" \
        "$BASE_URL/api/v1/users/profile" \
        15 \
        8 \
        "GET" \
        "-H 'X-User-ID: $PREMIUM_USER'"
}

# Function to test API endpoint performance
test_api_performance() {
    log "${YELLOW}🌐 API ENDPOINT PERFORMANCE TESTS${NC}"
    log "=================================="
    
    # Health endpoints (lightweight)
    run_concurrent_test "Health Check" \
        "$BASE_URL/api/v1/health" \
        50 \
        20 \
        "GET"
    
    run_concurrent_test "Detailed Health Check" \
        "$BASE_URL/api/v1/health/detailed" \
        30 \
        15 \
        "GET"
    
    # User management endpoints
    run_concurrent_test "User Profile Access" \
        "$BASE_URL/api/v1/users/profile" \
        25 \
        12 \
        "GET" \
        "-H 'X-User-ID: $PREMIUM_USER'"
    
    # Opportunity endpoints (data-heavy)
    run_concurrent_test "Opportunities Access" \
        "$BASE_URL/api/v1/opportunities" \
        20 \
        10 \
        "GET" \
        "-H 'X-User-ID: $PREMIUM_USER'"
    
    # Analytics endpoints (computation-heavy)
    run_concurrent_test "Analytics Dashboard" \
        "$BASE_URL/api/v1/analytics/dashboard" \
        15 \
        8 \
        "GET" \
        "-H 'X-User-ID: $ENTERPRISE_USER'"
    
    # AI endpoints (AI-heavy)
    local ai_payload='{"pair": "BTC/USDT", "exchanges": ["binance", "bybit"]}'
    run_concurrent_test "AI Analysis" \
        "$BASE_URL/api/v1/ai/analyze" \
        10 \
        5 \
        "POST" \
        "-H 'X-User-ID: $PREMIUM_USER'" \
        "$ai_payload"
}

# Function to test RBAC performance
test_rbac_performance() {
    log "${YELLOW}🔐 RBAC PERFORMANCE TESTS${NC}"
    log "========================="
    
    # Test different subscription tiers
    local users=("$FREE_USER" "$BASIC_USER" "$PREMIUM_USER" "$ENTERPRISE_USER" "$ADMIN_USER")
    
    for user in "${users[@]}"; do
        run_concurrent_test "RBAC Check - $user" \
            "$BASE_URL/api/v1/users/profile" \
            15 \
            10 \
            "GET" \
            "-H 'X-User-ID: $user'"
    done
}

# Function to test stress limits
test_stress_limits() {
    log "${YELLOW}💥 STRESS TESTING${NC}"
    log "================="
    
    # Gradually increase load
    local stress_levels=(10 25 50 75 100)
    
    for level in "${stress_levels[@]}"; do
        log "${BLUE}🔥 Stress Level: $level concurrent users${NC}"
        
        run_concurrent_test "Stress Test - $level users" \
            "$BASE_URL/api/v1/health" \
            $level \
            5 \
            "GET"
        
        # Brief pause between stress levels
        sleep 2
    done
}

# Function to test memory and resource usage
test_resource_usage() {
    log "${YELLOW}📊 RESOURCE USAGE TESTS${NC}"
    log "======================="
    
    # Test sustained load
    log "${BLUE}🔄 Sustained Load Test (30 seconds)${NC}"
    
    local start_time=$(date +%s)
    local end_time=$((start_time + STRESS_DURATION))
    local request_count=0
    local error_count=0
    
    while [[ $(date +%s) -lt $end_time ]]; do
        for _ in {1..5}; do
            {
                set +e  # Disable exit on error for background processes
                result=$(measure_response_time "$BASE_URL/api/v1/health")
                status_code=$(echo "$result" | cut -d: -f1)
                
                if [[ "$status_code" =~ ^[2-3][0-9][0-9]$ ]]; then
                    ((request_count++))
                else
                    ((error_count++))
                fi
                set -e  # Re-enable exit on error
            } &
        done
        wait
        sleep 0.1
    done
    
    local total_requests=$((request_count + error_count))
    local success_rate=$(( total_requests > 0 ? (request_count * 100) / total_requests : 0 ))
    
    log "${GREEN}✅ Sustained Load Results:${NC}"
    log "   📊 Total Requests: $total_requests"
    log "   ✅ Successful: $request_count ($success_rate%)"
    log "   ❌ Failed: $error_count"
    log "   ⏱️  Duration: ${STRESS_DURATION}s"
    log "   🚀 Average Throughput: $((total_requests / STRESS_DURATION)) req/sec"
    log ""
}

# Function to warm up the system
warmup_system() {
    log "${YELLOW}🔥 WARMING UP SYSTEM${NC}"
    log "==================="
    
    log "${BLUE}Sending $WARMUP_REQUESTS warmup requests...${NC}"
    
    for ((i=1; i<=WARMUP_REQUESTS; i++)); do
        curl -X GET "$BASE_URL/api/v1/health" -s -o /dev/null &
        if [[ $((i % 5)) -eq 0 ]]; then
            wait
        fi
    done
    wait
    
    log "${GREEN}✅ System warmed up${NC}"
    log ""
}

# Function to generate performance report
generate_performance_report() {
    log "${PURPLE}📋 PERFORMANCE SUMMARY REPORT${NC}"
    log "=============================="
    
    log "${CYAN}🎯 Test Configuration:${NC}"
    log "   Base URL: $BASE_URL"
    log "   Max Concurrent Users: $CONCURRENT_USERS"
    log "   Requests per User: $REQUESTS_PER_USER"
    log "   Stress Duration: ${STRESS_DURATION}s"
    log ""
    
    log "${CYAN}📊 Response Time Summary:${NC}"
    for test_name in "${!response_times[@]}"; do
        log "   $test_name: ${response_times[$test_name]}ms"
    done
    log ""
    
    log "${CYAN}✅ Success Rate Summary:${NC}"
    for test_name in "${!success_counts[@]}"; do
        local total=$((success_counts[$test_name] + error_counts[$test_name]))
        local rate=$(( total > 0 ? (success_counts[$test_name] * 100) / total : 0 ))
        log "   $test_name: $rate% (${success_counts[$test_name]}/${total})"
    done
    log ""
    
    # Performance recommendations
    log "${CYAN}💡 Performance Recommendations:${NC}"
    
    local avg_response_time=0
    local test_count=0
    for time in "${response_times[@]}"; do
        avg_response_time=$((avg_response_time + time))
        ((test_count++))
    done
    
    if [[ $test_count -gt 0 ]]; then
        avg_response_time=$((avg_response_time / test_count))
        
        if [[ $avg_response_time -lt 100 ]]; then
            log "   🟢 Excellent: Average response time under 100ms"
        elif [[ $avg_response_time -lt 300 ]]; then
            log "   🟡 Good: Average response time under 300ms"
        elif [[ $avg_response_time -lt 1000 ]]; then
            log "   🟠 Fair: Average response time under 1s - consider optimization"
        else
            log "   🔴 Poor: Average response time over 1s - optimization needed"
        fi
    fi
    
    log "   📈 Consider implementing caching for frequently accessed endpoints"
    log "   🔄 Monitor service injection overhead in production"
    log "   📊 Set up performance monitoring and alerting"
    log "   🚀 Consider connection pooling for database operations"
}

# Main execution
main() {
    log "${PURPLE}🚀 ArbEdge Comprehensive Performance Testing${NC}"
    log "============================================="
    log "Base URL: $BASE_URL"
    log "Timestamp: $(date '+%Y%m%d_%H%M%S')"
    log "============================================="
    log ""
    
    # Check if server is running
    if ! curl -s "$BASE_URL/api/v1/health" > /dev/null; then
        log "${RED}❌ Server not responding at $BASE_URL${NC}"
        log "${YELLOW}Please ensure the server is running before running performance tests${NC}"
        exit 1
    fi
    
    # Warm up system
    warmup_system
    
    # Run performance tests
    test_service_injection_performance
    test_api_performance
    test_rbac_performance
    test_stress_limits
    test_resource_usage
    
    # Generate report
    generate_performance_report
    
    log "${GREEN}🎉 Performance testing completed!${NC}"
}

# Run main function
main "$@" 
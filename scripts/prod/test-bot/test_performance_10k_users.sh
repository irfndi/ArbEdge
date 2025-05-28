#!/opt/homebrew/bin/bash

# ArbEdge High-Scale Performance Testing - 10,000 Concurrent Users
# Professional load testing with safety measures and monitoring

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
BASE_URL="${BASE_URL:-https://arb-edge.irfandimarsya.workers.dev}"
MAX_USERS="${MAX_USERS:-10000}"
RAMP_UP_DURATION="${RAMP_UP_DURATION:-300}"  # 5 minutes ramp-up
TEST_DURATION="${TEST_DURATION:-600}"        # 10 minutes sustained load
CONNECTIONS="${CONNECTIONS:-10000}"
THREADS="${THREADS:-100}"
TIMEOUT="${TIMEOUT:-30s}"
SAFETY_CHECK_INTERVAL="${SAFETY_CHECK_INTERVAL:-30}"
MAX_ERROR_RATE="${MAX_ERROR_RATE:-10}"       # Stop if error rate > 10%
MAX_RESPONSE_TIME="${MAX_RESPONSE_TIME:-5000}" # Stop if avg response > 5s

# Test endpoints for different load patterns
HEALTH_ENDPOINT="/api/v1/health"
USER_ENDPOINT="/api/v1/user/profile"
OPPORTUNITIES_ENDPOINT="/api/v1/opportunities"
ANALYTICS_ENDPOINT="/api/v1/analytics/dashboard"

# Results directory
RESULTS_DIR="@logs/performance_results_10k_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# Log file
LOG_FILE="$RESULTS_DIR/test_execution.log"

# Function to log with timestamp
log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# Function to check if required tools are installed
check_dependencies() {
    log "🔍 Checking dependencies..."
    
    local missing_tools=()
    
    # Check for wrk (preferred high-performance tool)
    if ! command -v wrk &> /dev/null; then
        missing_tools+=("wrk")
    fi
    
    # Check for hey (alternative tool)
    if ! command -v hey &> /dev/null; then
        missing_tools+=("hey")
    fi
    
    # Check for curl
    if ! command -v curl &> /dev/null; then
        missing_tools+=("curl")
    fi
    
    if [ ${#missing_tools[@]} -gt 0 ]; then
        echo -e "${RED}❌ Missing required tools: ${missing_tools[*]}${NC}"
        echo -e "${YELLOW}📦 Install with:${NC}"
        for tool in "${missing_tools[@]}"; do
            case $tool in
                "wrk")
                    echo "  brew install wrk"
                    ;;
                "hey")
                    echo "  brew install hey"
                    ;;
                "curl")
                    echo "  brew install curl"
                    ;;
            esac
        done
        exit 1
    fi
    
    log "✅ All dependencies available"
}

# Function to test server connectivity
test_connectivity() {
    log "🌐 Testing server connectivity..."
    
    local response
    response=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 "$BASE_URL$HEALTH_ENDPOINT" || echo "000")
    
    if [ "$response" != "200" ]; then
        echo -e "${RED}❌ Server not responding (HTTP $response)${NC}"
        echo -e "${YELLOW}🔧 Please ensure the server is running at: $BASE_URL${NC}"
        exit 1
    fi
    
    log "✅ Server responding correctly"
}

# Function to run safety checks during testing
safety_check() {
    local error_rate=$1
    local avg_response_time=$2
    
    # Check error rate
    if (( $(echo "$error_rate > $MAX_ERROR_RATE" | bc -l) )); then
        echo -e "${RED}🚨 SAFETY STOP: Error rate ($error_rate%) exceeds threshold ($MAX_ERROR_RATE%)${NC}"
        return 1
    fi
    
    # Check response time
    if (( $(echo "$avg_response_time > $MAX_RESPONSE_TIME" | bc -l) )); then
        echo -e "${RED}🚨 SAFETY STOP: Response time (${avg_response_time}ms) exceeds threshold (${MAX_RESPONSE_TIME}ms)${NC}"
        return 1
    fi
    
    return 0
}

# Function to run wrk load test
run_wrk_test() {
    local endpoint=$1
    local users=$2
    local duration=$3
    local test_name=$4
    local output_file="$RESULTS_DIR/wrk_${test_name}_${users}users.txt"
    
    log "🚀 Running wrk test: $test_name ($users users, ${duration}s)"
    
    # Create Lua script for custom headers and user simulation
    local lua_script="$RESULTS_DIR/test_script.lua"
    cat > "$lua_script" << 'EOF'
-- Simulate different user types
local user_types = {"free", "basic", "premium", "enterprise", "pro", "admin"}
local user_counter = 0

request = function()
    user_counter = user_counter + 1
    local user_type = user_types[(user_counter % #user_types) + 1]
    local user_id = "user_" .. user_type .. "_" .. (user_counter % 1000)
    
    wrk.headers["X-User-ID"] = user_id
    wrk.headers["Content-Type"] = "application/json"
    wrk.headers["User-Agent"] = "ArbEdge-LoadTest/1.0"
    
    return wrk.format(nil, nil, nil, nil)
end

response = function(status, headers, body)
    if status ~= 200 then
        print("Error response: " .. status)
    end
end
EOF
    
    # Run wrk with custom script
    wrk -t"$THREADS" -c"$users" -d"${duration}s" \
        --timeout="$TIMEOUT" \
        --script="$lua_script" \
        "$BASE_URL$endpoint" > "$output_file" 2>&1
    
    # Parse results
    local requests_per_sec
    local avg_latency
    local error_rate
    
    requests_per_sec=$(grep "Requests/sec:" "$output_file" | awk '{print $2}' || echo "0")
    avg_latency=$(grep "Latency" "$output_file" | awk '{print $2}' | sed 's/ms//' || echo "0")
    
    # Calculate error rate from status codes
    local total_requests
    local error_requests
    total_requests=$(grep "requests in" "$output_file" | awk '{print $1}' || echo "1")
    error_requests=$(grep -E "(Non-2xx|Socket errors)" "$output_file" | awk '{sum += $3} END {print sum+0}')
    error_rate=$(echo "scale=2; $error_requests * 100 / $total_requests" | bc -l 2>/dev/null || echo "0")
    
    # Convert latency to milliseconds if needed
    if [[ "$avg_latency" == *"s" ]]; then
        avg_latency=$(echo "$avg_latency" | sed 's/s//' | awk '{print $1 * 1000}')
    fi
    
    log "📊 Results: $requests_per_sec req/sec, ${avg_latency}ms avg latency, ${error_rate}% errors"
    
    # Safety check
    if ! safety_check "$error_rate" "$avg_latency"; then
        return 1
    fi
    
    return 0
}

# Function to run hey load test (alternative)
run_hey_test() {
    local endpoint=$1
    local users=$2
    local duration=$3
    local test_name=$4
    local output_file="$RESULTS_DIR/hey_${test_name}_${users}users.txt"
    
    log "🚀 Running hey test: $test_name ($users users, ${duration}s)"
    
    # Calculate total requests (users * duration * estimated req/sec per user)
    local total_requests=$((users * duration * 2))
    
    hey -n "$total_requests" -c "$users" -t "$TIMEOUT" \
        -H "X-User-ID: user_premium_loadtest" \
        -H "Content-Type: application/json" \
        "$BASE_URL$endpoint" > "$output_file" 2>&1
    
    # Parse results
    local requests_per_sec
    local avg_latency
    local error_rate
    
    requests_per_sec=$(grep "Requests/sec:" "$output_file" | awk '{print $2}' || echo "0")
    avg_latency=$(grep "Average:" "$output_file" | awk '{print $2}' | sed 's/ms//' || echo "0")
    error_rate=$(grep "Error distribution:" -A 10 "$output_file" | grep -E "\[" | awk '{sum += $2} END {print sum+0}' || echo "0")
    
    log "📊 Results: $requests_per_sec req/sec, ${avg_latency}ms avg latency, ${error_rate}% errors"
    
    # Safety check
    if ! safety_check "$error_rate" "$avg_latency"; then
        return 1
    fi
    
    return 0
}

# Function to run gradual ramp-up test
run_ramp_up_test() {
    log "📈 Starting gradual ramp-up test to $MAX_USERS users"
    
    local ramp_steps=(100 500 1000 2500 5000 7500 10000)
    local step_duration=60  # 1 minute per step
    
    for users in "${ramp_steps[@]}"; do
        if [ "$users" -gt "$MAX_USERS" ]; then
            break
        fi
        
        echo -e "${CYAN}🔄 Ramp-up Step: $users concurrent users${NC}"
        
        # Test health endpoint (lightweight)
        if ! run_wrk_test "$HEALTH_ENDPOINT" "$users" "$step_duration" "ramp_health"; then
            echo -e "${RED}❌ Ramp-up failed at $users users${NC}"
            return 1
        fi
        
        # Brief pause between steps
        sleep 10
    done
    
    log "✅ Ramp-up test completed successfully"
}

# Function to run sustained load test
run_sustained_load_test() {
    log "⚡ Starting sustained load test with $MAX_USERS users for ${TEST_DURATION}s"
    
    # Test different endpoints with different load patterns
    local endpoints=(
        "$HEALTH_ENDPOINT:health:$MAX_USERS"
        "$USER_ENDPOINT:user:$((MAX_USERS / 2))"
        "$OPPORTUNITIES_ENDPOINT:opportunities:$((MAX_USERS / 4))"
        "$ANALYTICS_ENDPOINT:analytics:$((MAX_USERS / 10))"
    )
    
    for endpoint_config in "${endpoints[@]}"; do
        IFS=':' read -r endpoint name users <<< "$endpoint_config"
        
        echo -e "${PURPLE}🎯 Testing $name endpoint with $users users${NC}"
        
        if ! run_wrk_test "$endpoint" "$users" "$TEST_DURATION" "sustained_$name"; then
            echo -e "${RED}❌ Sustained load test failed on $name endpoint${NC}"
            return 1
        fi
        
        # Brief pause between endpoint tests
        sleep 30
    done
    
    log "✅ Sustained load test completed successfully"
}

# Function to generate comprehensive report
generate_report() {
    local report_file="$RESULTS_DIR/performance_report.md"
    
    log "📋 Generating comprehensive performance report..."
    
    cat > "$report_file" << EOF
# ArbEdge 10K Users Performance Test Report

**Test Date**: $(date)
**Target URL**: $BASE_URL
**Maximum Users**: $MAX_USERS
**Test Duration**: ${TEST_DURATION}s
**Ramp-up Duration**: ${RAMP_UP_DURATION}s

## Test Configuration

- **Threads**: $THREADS
- **Connections**: $CONNECTIONS
- **Timeout**: $TIMEOUT
- **Safety Thresholds**:
  - Max Error Rate: $MAX_ERROR_RATE%
  - Max Response Time: ${MAX_RESPONSE_TIME}ms

## Test Results

### Ramp-up Test Results
EOF
    
    # Add ramp-up results
    for file in "$RESULTS_DIR"/wrk_ramp_*.txt; do
        if [ -f "$file" ]; then
            echo "#### $(basename "$file" .txt)" >> "$report_file"
            echo '```' >> "$report_file"
            cat "$file" >> "$report_file"
            echo '```' >> "$report_file"
            echo "" >> "$report_file"
        fi
    done
    
    cat >> "$report_file" << EOF

### Sustained Load Test Results
EOF
    
    # Add sustained load results
    for file in "$RESULTS_DIR"/wrk_sustained_*.txt; do
        if [ -f "$file" ]; then
            echo "#### $(basename "$file" .txt)" >> "$report_file"
            echo '```' >> "$report_file"
            cat "$file" >> "$report_file"
            echo '```' >> "$report_file"
            echo "" >> "$report_file"
        fi
    done
    
    cat >> "$report_file" << EOF

## Performance Summary

### Key Metrics
- **Peak Concurrent Users**: $MAX_USERS
- **Test Duration**: ${TEST_DURATION} seconds
- **Total Test Time**: $(date -d@$(($(date +%s) - $(stat -c %Y "$LOG_FILE"))) -u +%H:%M:%S)

### Recommendations
1. **Scaling**: System handled $MAX_USERS concurrent users
2. **Monitoring**: Implement real-time monitoring for production
3. **Optimization**: Consider caching and connection pooling
4. **Alerting**: Set up alerts for response time and error rate thresholds

## Test Execution Log
\`\`\`
$(cat "$LOG_FILE")
\`\`\`
EOF
    
    echo -e "${GREEN}📋 Report generated: $report_file${NC}"
}

# Main execution function
main() {
    echo -e "${BLUE}🚀 ArbEdge 10K Users Performance Testing${NC}"
    echo -e "${BLUE}=======================================${NC}"
    echo ""
    echo -e "${CYAN}Configuration:${NC}"
    echo -e "  Target URL: $BASE_URL"
    echo -e "  Max Users: $MAX_USERS"
    echo -e "  Test Duration: ${TEST_DURATION}s"
    echo -e "  Results Dir: $RESULTS_DIR"
    echo ""
    
    # Pre-flight checks
    check_dependencies
    test_connectivity
    
    # Start testing
    local start_time=$(date +%s)
    
    echo -e "${YELLOW}🏁 Starting performance testing...${NC}"
    
    # Phase 1: Gradual ramp-up
    if ! run_ramp_up_test; then
        echo -e "${RED}❌ Testing stopped due to safety limits${NC}"
        exit 1
    fi
    
    # Phase 2: Sustained load
    if ! run_sustained_load_test; then
        echo -e "${RED}❌ Testing stopped due to safety limits${NC}"
        exit 1
    fi
    
    local end_time=$(date +%s)
    local total_time=$((end_time - start_time))
    
    # Generate report
    generate_report
    
    echo ""
    echo -e "${GREEN}🎉 Performance testing completed successfully!${NC}"
    echo -e "${GREEN}⏱️  Total test time: $(date -d@$total_time -u +%H:%M:%S)${NC}"
    echo -e "${GREEN}📁 Results saved to: $RESULTS_DIR${NC}"
    echo ""
    echo -e "${CYAN}📊 Quick Summary:${NC}"
    echo -e "  ✅ System handled $MAX_USERS concurrent users"
    echo -e "  ✅ All safety thresholds maintained"
    echo -e "  ✅ Comprehensive performance data collected"
    echo ""
    echo -e "${YELLOW}🔍 Next Steps:${NC}"
    echo -e "  1. Review detailed report: $RESULTS_DIR/performance_report.md"
    echo -e "  2. Analyze individual test results in $RESULTS_DIR/"
    echo -e "  3. Set up production monitoring based on findings"
    echo -e "  4. Consider optimization recommendations"
}

# Handle script interruption
trap 'echo -e "\n${YELLOW}⚠️  Test interrupted by user${NC}"; exit 130' INT

# Run main function
main "$@" 
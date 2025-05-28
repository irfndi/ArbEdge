#!/opt/homebrew/bin/bash

# Minimal Performance Testing Script
set -e

BASE_URL="${BASE_URL:-http://localhost:8787}"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

log() {
    echo -e "$1"
}

# Minimal concurrent test using the exact working structure
minimal_concurrent_test() {
    local test_name="$1"
    local url="$2"
    local concurrent_users="$3"
    local requests_per_user="$4"
    
    log "${CYAN}🚀 Running $test_name${NC}"
    log "${BLUE}   Concurrent Users: $concurrent_users, Requests per User: $requests_per_user${NC}"
    
    temp_dir=$(mktemp -d)
    local start_time=$(date +%s%N)
    
    # Launch concurrent users using the exact working structure
    for ((user=1; user<=concurrent_users; user++)); do
        {
            set +e  # Disable exit on error for background processes
            user_success=0
            user_errors=0
            user_total_time=0
            
            for ((req=1; req<=requests_per_user; req++)); do
                start_req=$(date +%s%N)
                result=$(curl -X GET "$url" -s -w "%{http_code}" -o /dev/null 2>/dev/null)
                end_req=$(date +%s%N)
                response_time=$(( (end_req - start_req) / 1000000 ))
                
                if [[ "$result" == "200" ]]; then
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
    
    # Wait for completion
    wait
    
    local end_time=$(date +%s%N)
    local total_duration=$(( (end_time - start_time) / 1000000 ))
    
    # Aggregate results using the exact working structure
    local total_success=0
    local total_errors=0
    local total_response_time=0
    
    for ((user=1; user<=concurrent_users; user++)); do
        if [[ -f "$temp_dir/user_$user.result" ]]; then
            result=$(cat "$temp_dir/user_$user.result")
            user_success=$(echo "$result" | cut -d: -f1)
            user_errors=$(echo "$result" | cut -d: -f2)
            user_time=$(echo "$result" | cut -d: -f3)
            
            total_success=$((total_success + user_success))
            total_errors=$((total_errors + user_errors))
            total_response_time=$((total_response_time + user_time))
        else
            log "❌ User $user result file missing"
        fi
    done
    
    # Calculate metrics
    local total_requests=$((total_success + total_errors))
    local success_rate=$(( total_requests > 0 ? (total_success * 100) / total_requests : 0 ))
    local avg_response_time=$(( total_requests > 0 ? total_response_time / total_requests : 0 ))
    local throughput=$(( total_duration > 0 ? (total_requests * 1000) / total_duration : 0 ))
    
    # Display results
    log "${GREEN}✅ Results:${NC}"
    log "   📊 Total Requests: $total_requests"
    log "   ✅ Successful: $total_success ($success_rate%)"
    log "   ❌ Failed: $total_errors"
    log "   ⏱️  Average Response Time: ${avg_response_time}ms"
    log "   🚀 Throughput: ${throughput} req/sec"
    log "   ⏰ Total Duration: ${total_duration}ms"
    log ""
    
    # Cleanup
    rm -rf "$temp_dir"
}

# Main execution
main() {
    log "${CYAN}🚀 Minimal Performance Test${NC}"
    log "Base URL: $BASE_URL"
    log ""
    
    # Check if server is running
    if ! curl -s "$BASE_URL/api/v1/health" > /dev/null; then
        log "❌ Server not responding at $BASE_URL"
        exit 1
    fi
    
    # Run minimal tests
    minimal_concurrent_test "Health Check Test" "$BASE_URL/api/v1/health" 3 2
    minimal_concurrent_test "User Profile Test" "$BASE_URL/api/v1/users/profile" 2 2
    
    log "${GREEN}🎉 Minimal performance testing completed!${NC}"
}

main "$@" 
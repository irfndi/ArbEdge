#!/bin/bash

# Production Telegram Bot Test Script
# Tests the production Telegram webhook to check opportunities response

set -e

# Configuration
WEBHOOK_URL="https://arb-edge.irfandimarsya.workers.dev/telegram/webhook"
USER_ID="DevTest"
CHAT_ID="123456789"
USERNAME="devtestuser"

echo "🧪 Testing PRODUCTION Telegram Bot"
echo "=================================="
echo "🎯 Target URL: $WEBHOOK_URL"
echo ""

# Function to send Telegram webhook request
send_telegram_request() {
    local command="$1"
    local test_name="$2"
    
    echo "📚 Test: $test_name"
    echo "----------------------------------------------------------"
    echo "Sending $command command..."
    
    # Create Telegram webhook payload
    local payload=$(cat <<EOF
{
  "update_id": 123456789,
  "message": {
    "message_id": 1,
    "from": {
      "id": $USER_ID,
      "is_bot": false,
      "first_name": "DevTest",
      "username": "$USERNAME",
      "language_code": "en"
    },
    "chat": {
      "id": $CHAT_ID,
      "first_name": "DevTest",
      "username": "$USERNAME",
      "type": "private"
    },
    "date": $(date +%s),
    "text": "$command"
  }
}
EOF
)
    
    # Send request and capture response
    local response=$(curl -s -X POST "$WEBHOOK_URL" \
        -H "Content-Type: application/json" \
        -d "$payload" \
        --max-time 30 \
        --connect-timeout 10 \
        2>/dev/null || echo "ERROR: Request failed")
    
    echo "Response: $response"
    echo ""
    
    # Check if response contains opportunities
    if [[ "$command" == "/opportunities" ]]; then
        if echo "$response" | grep -q "opportunities"; then
            echo "✅ Found opportunities in response!"
            # Count opportunities if possible
            local opp_count=$(echo "$response" | grep -o "opportunities" | wc -l)
            echo "📊 Opportunities mentioned: $opp_count times"
        else
            echo "❌ No opportunities found in response"
        fi
    fi
    
    return 0
}

# Test commands
send_telegram_request "/help" "Testing /help command (should show command list)"
send_telegram_request "/opportunities" "Testing /opportunities command (should show trading opportunities)"
send_telegram_request "/profile" "Testing /profile command (should show user profile)"

echo "✅ Testing completed!"
echo "===================="
echo ""
echo "Expected Results:"
echo "- /help should show a list of available commands"
echo "- /opportunities should show real trading opportunities or appropriate messages"
echo "- /profile should show user profile information or setup prompts"
echo ""
echo "🔍 Check the responses above to see if opportunities are being returned"
echo "📊 If /opportunities shows 100+ opportunities, that indicates mock data issue" 
# ArbEdge API Testing Framework

This testing framework validates the complete user journey including RBAC (Role-Based Access Control) and Subscription logic to ensure our Telegram bot UX matches our intentions without manual testing.

## Overview

The API testing framework consists of:

1. **`test_api_flow.sh`** - Main test script that validates all API endpoints
2. **`test_config.json`** - Configuration defining expected behavior for each subscription tier
3. **Makefile targets** - Easy-to-use commands for running tests against different environments

## Quick Start

```bash
# Run tests against local development server
make test-api-local

# Run tests against staging environment
make test-api-staging

# Run tests against production environment
make test-api-production

# Run tests with custom URL
BASE_URL=https://arb-edge.irfandimarsya.workers.dev make test-api
```

## Test Categories

### 1. Health Check Tests
- Basic health endpoint validation
- Detailed health check with service status
- Response format validation

### 2. Authentication Tests
- Unauthorized access handling
- Invalid user ID rejection
- Authentication header validation

### 3. User Profile & RBAC Tests
Tests user profile access across different subscription tiers:
- **Free User**: Basic profile access, limited features
- **Premium User**: Enhanced profile features, premium access
- **Pro User**: Full profile features, analytics access
- **Admin User**: Complete system access, user management

### 4. Opportunity Access & Subscription Tests
Validates subscription-based feature access:
- **Free Tier**: 5 opportunities max, no premium features
- **Premium Tier**: 20 opportunities max, premium features enabled
- **Pro Tier**: 50 opportunities max, dashboard access
- **Admin Tier**: 100 opportunities max, full system access

### 5. Opportunity Execution Tests
Tests execution limits based on subscription:
- Rate limiting enforcement
- Execution quotas per tier
- Error handling for exceeded limits

### 6. Analytics Access Tests
Validates analytics access permissions:
- Free/Premium: No dashboard access
- Pro/Admin: Full dashboard access
- User-specific analytics for all tiers

### 7. Rate Limiting Tests
Tests rate limiting enforcement:
- Different limits per subscription tier
- Rate limit recovery
- Proper error responses (429)

### 8. Telegram Bot Flow Simulation
Simulates actual Telegram webhook payloads:
- `/start` command handling
- `/opportunities` command
- `/profile` command
- Callback query handling

### 9. Error Handling Tests
Validates proper error responses:
- Invalid JSON payloads (400)
- Missing required fields (400)
- Non-existent endpoints (404)
- Unauthorized access (401)
- Insufficient permissions (403)

### 10. Performance Tests
Tests system performance:
- Concurrent request handling
- Response time validation
- Load testing scenarios

## Subscription Tier Matrix

| Feature | Free | Premium | Pro | Admin |
|---------|------|---------|-----|-------|
| Opportunities/hour | 5 | 20 | 50 | 100 |
| Premium Features | ❌ | ✅ | ✅ | ✅ |
| Analytics Dashboard | ❌ | ❌ | ✅ | ✅ |
| User Analytics | ✅ | ✅ | ✅ | ✅ |
| Rate Limit (req/min) | 10 | 30 | 60 | 120 |
| User Management | ❌ | ❌ | ❌ | ✅ |

## Test Users

The framework uses predefined test users for each subscription tier:

```json
{
  "free_user": "user_free_123",
  "premium_user": "user_premium_456", 
  "pro_user": "user_pro_789",
  "admin_user": "user_admin_000"
}
```

## Expected Behaviors

### RBAC Enforcement
- Free users blocked from premium features
- Premium users have enhanced access
- Pro users get dashboard access
- Admin users have full system control

### Rate Limiting
- **Free**: Strict limits (10 req/min)
- **Premium**: Moderate limits (30 req/min)
- **Pro**: Relaxed limits (60 req/min)
- **Admin**: Minimal limits (120 req/min)

### Error Responses
- **401**: Authentication required
- **403**: Upgrade subscription for access
- **429**: Rate limit exceeded
- **400**: Invalid request format

## Telegram Bot Flow Validation

The tests simulate real Telegram bot interactions:

### User Registration Flow
1. Send `/start` command → Welcome message with registration prompt
2. Provide invitation code → Registration success/error
3. Setup preferences → Preferences saved confirmation

### Opportunity Discovery Flow
1. Send `/opportunities` command → List based on subscription
2. Click opportunity details → Detailed view
3. Execute opportunity → Confirmation or limit reached

### Profile Management Flow
1. Send `/profile` command → User profile display
2. Update risk tolerance → Preferences updated
3. View subscription status → Details and limits

## Output and Logging

Test results are saved to:
- `./test_results/api_test_YYYYMMDD_HHMMSS.log` - Detailed test log
- `./test_results/response_N.json` - Individual API responses

## Integration with CI/CD

The API tests can be integrated into your CI/CD pipeline:

```yaml
# Example GitHub Actions integration
- name: Run API Tests
  run: |
    make test-api-staging
    if [ $? -eq 0 ]; then
      echo "✅ API tests passed - ready for production"
    else
      echo "❌ API tests failed - blocking deployment"
      exit 1
    fi
```

## Customization

### Adding New Test Scenarios

1. **Add test function** in `test_api_flow.sh`:
```bash
run_test "New Feature Test" "200" \
    "curl -X GET '$BASE_URL/api/v1/new-feature' -H 'X-User-ID: $user_id'" \
    "validate_new_feature_response"
```

2. **Add validation function**:
```bash
validate_new_feature_response() {
    local response_file="$1"
    jq -e '.success == true and .data.new_field != null' "$response_file" >/dev/null
}
```

3. **Update test configuration** in `test_config.json`:
```json
{
  "new_feature": {
    "free_tier": false,
    "premium_tier": true,
    "expected_response": "feature_data"
  }
}
```

### Environment Variables

- `BASE_URL`: Target API URL (default: staging)
- `TEST_OUTPUT_DIR`: Output directory for test results
- `TIMEOUT`: Request timeout in seconds

## Troubleshooting

### Common Issues

1. **Tests failing with 000 status code**
   - Check if the target URL is accessible
   - Verify network connectivity
   - Ensure the service is running

2. **Authentication errors**
   - Verify test user IDs are configured correctly
   - Check if authentication headers are being sent

3. **Rate limiting issues**
   - Wait for rate limit reset window
   - Use different test users for parallel testing

### Debug Mode

Run tests with debug output:
```bash
DEBUG=1 make test-api-local
```

## Contributing

When adding new features to the API:

1. Update the test script with new test cases
2. Add expected behaviors to `test_config.json`
3. Update this README with new test categories
4. Ensure all tests pass before merging

## Security Considerations

- Test users should only exist in development/staging environments
- Production tests should use read-only operations
- API keys and sensitive data should never be committed to the repository
- Rate limiting tests should be run carefully to avoid service disruption

---

This testing framework ensures that our Telegram bot provides a consistent user experience across all subscription tiers while properly enforcing access controls and rate limits. 
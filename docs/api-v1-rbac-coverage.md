# ArbEdge API v1 RBAC Coverage Documentation

## Subscription Tier Hierarchy

```
Free < Basic < Premium < Enterprise < SuperAdmin
```

## Subscription Tier Mapping

| User ID Pattern | Subscription Tier | Access Level |
|----------------|------------------|--------------|
| `user_free_*` | Free | `free_without_api` |
| `user_basic_*` | Basic | `free_with_api` |
| `user_premium_*` | Premium | `subscription_with_api` |
| `user_enterprise_*` | Enterprise | `subscription_with_api` |
| `user_pro_*` | Enterprise (legacy) | `subscription_with_api` |
| `user_admin_*` | SuperAdmin | `subscription_with_api` |

## API v1 Endpoint Coverage

### 🟢 Public Endpoints (No Authentication Required)

| Endpoint | Method | Description | Access |
|----------|--------|-------------|--------|
| `/api/v1/health` | GET | Basic health check | Public |
| `/api/v1/health/detailed` | GET | Detailed service status | Public |

### 🔵 User Management Endpoints (All Authenticated Users)

| Endpoint | Method | Required Tier | Description |
|----------|--------|---------------|-------------|
| `/api/v1/users/profile` | GET | `free` | Get user profile |
| `/api/v1/users/profile` | PUT | `free` | Update user profile |
| `/api/v1/users/preferences` | GET | `free` | Get user preferences |
| `/api/v1/users/preferences` | PUT | `free` | Update user preferences |

### 🟡 Opportunity Endpoints (Subscription-Based Limits)

| Endpoint | Method | Required Tier | Limits | Description |
|----------|--------|---------------|--------|-------------|
| `/api/v1/opportunities` | GET | `free` | 5 opportunities | Basic opportunities |
| `/api/v1/opportunities?premium=true` | GET | `premium` | 20 opportunities | Premium opportunities |
| `/api/v1/opportunities/execute` | POST | `free` | Basic execution | Execute opportunity |

**Opportunity Limits by Tier:**
- **Free**: 5 opportunities
- **Basic**: 10 opportunities  
- **Premium**: 20 opportunities
- **Enterprise**: 50 opportunities
- **SuperAdmin**: 100 opportunities

### 🟠 Analytics Endpoints (Enterprise+ Only)

| Endpoint | Method | Required Tier | Description |
|----------|--------|---------------|-------------|
| `/api/v1/analytics/dashboard` | GET | `enterprise` | Dashboard analytics |
| `/api/v1/analytics/system` | GET | `admin` | System analytics (Admin only) |
| `/api/v1/analytics/users` | GET | `enterprise` | User analytics |
| `/api/v1/analytics/performance` | GET | `enterprise` | Performance analytics |
| `/api/v1/analytics/user` | GET | `free` | User-specific analytics |

### 🔴 Admin Endpoints (SuperAdmin Only)

| Endpoint | Method | Required Tier | Description |
|----------|--------|---------------|-------------|
| `/api/v1/admin/users` | GET | `admin` | Get all users |
| `/api/v1/admin/sessions` | GET | `admin` | Get user sessions |
| `/api/v1/admin/opportunities` | GET | `admin` | Get opportunities |
| `/api/v1/admin/user-profiles` | GET | `admin` | Get user profiles |
| `/api/v1/admin/manage/users` | GET | `admin` | User management |
| `/api/v1/admin/config/system` | GET | `admin` | System configuration |
| `/api/v1/admin/invitations` | GET | `admin` | Invitation management |

### 🟣 Trading Endpoints (Premium+ Only)

| Endpoint | Method | Required Tier | Description |
|----------|--------|---------------|-------------|
| `/api/v1/trading/balance` | GET | `premium` | Get trading balance |
| `/api/v1/trading/markets` | GET | `premium` | Get trading markets |
| `/api/v1/trading/opportunities` | GET | `premium` | Get trading opportunities |

### 🔮 AI Endpoints (Premium+ Only)

| Endpoint | Method | Required Tier | Description |
|----------|--------|---------------|-------------|
| `/api/v1/ai/analyze` | POST | `premium` | AI market analysis |
| `/api/v1/ai/risk-assessment` | POST | `premium` | AI risk assessment |

## User Journey Coverage

### Free User Journey ✅
- ✅ Health checks
- ✅ User profile management
- ✅ Basic opportunities (5 limit)
- ✅ Basic opportunity execution
- ✅ Personal analytics
- ❌ Premium opportunities (403 Forbidden)
- ❌ Trading features (403 Forbidden)
- ❌ AI features (403 Forbidden)
- ❌ Analytics dashboard (403 Forbidden)
- ❌ Admin features (403 Forbidden)

### Basic User Journey ✅
- ✅ All Free tier features
- ✅ Increased opportunity limit (10)
- ❌ Premium opportunities (403 Forbidden)
- ❌ Trading features (403 Forbidden)
- ❌ AI features (403 Forbidden)
- ❌ Analytics dashboard (403 Forbidden)
- ❌ Admin features (403 Forbidden)

### Premium User Journey ✅
- ✅ All Basic tier features
- ✅ Premium opportunities (20 limit)
- ✅ Trading features
- ✅ AI features
- ❌ Analytics dashboard (403 Forbidden)
- ❌ Admin features (403 Forbidden)

### Enterprise User Journey ✅
- ✅ All Premium tier features
- ✅ High opportunity limit (50)
- ✅ Analytics dashboard
- ✅ Performance analytics
- ✅ User analytics
- ❌ System analytics (403 Forbidden)
- ❌ Admin features (403 Forbidden)

### SuperAdmin User Journey ✅
- ✅ All Enterprise tier features
- ✅ Highest opportunity limit (100)
- ✅ System analytics
- ✅ All admin endpoints
- ✅ User management
- ✅ System configuration
- ✅ Invitation management

## Authentication & Authorization

### Authentication Method
- **Header**: `X-User-ID: {user_id}`
- **Fallback**: Pattern-based tier detection for testing
- **Production**: D1 database lookup with proper RBAC

### Authorization Flow
1. Extract `X-User-ID` from request headers
2. Attempt D1 database lookup for user profile
3. Fallback to pattern-based tier detection
4. Check subscription tier permissions
5. Return 401 for missing auth, 403 for insufficient permissions

### Error Responses

| Status Code | Condition | Response |
|-------------|-----------|----------|
| 401 | Missing `X-User-ID` header | `{"success": false, "error": "Authentication required"}` |
| 403 | Insufficient subscription tier | `{"success": false, "error": "Upgrade subscription for access"}` |
| 403 | Admin access required | `{"success": false, "error": "Admin access required"}` |

## Test Coverage

### Test Users
```bash
FREE_USER="user_free_123"
BASIC_USER="user_basic_234"
PREMIUM_USER="user_premium_456"
ENTERPRISE_USER="user_enterprise_678"
PRO_USER="user_pro_789"  # Maps to Enterprise
ADMIN_USER="user_admin_000"
```

### Test Scenarios
- ✅ Authentication validation (401 errors)
- ✅ Authorization validation (403 errors)
- ✅ Subscription tier progression
- ✅ Opportunity limit enforcement
- ✅ Feature access control
- ✅ Admin privilege separation
- ✅ Response format validation

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| RBAC Integration | ✅ Complete | Proper D1 database lookup with fallback |
| Subscription Tiers | ✅ Complete | All 5 tiers implemented |
| Endpoint Coverage | ✅ Complete | 25 endpoints across 6 categories |
| Test Coverage | ✅ Complete | Comprehensive test script |
| Error Handling | ✅ Complete | Proper HTTP status codes |
| Documentation | ✅ Complete | This document |

## Next Steps

1. **Production Deployment**: Deploy with proper D1 database
2. **Rate Limiting**: Implement tier-based rate limiting
3. **Monitoring**: Add endpoint usage analytics
4. **Performance**: Optimize database queries
5. **Security**: Add request signing/validation 
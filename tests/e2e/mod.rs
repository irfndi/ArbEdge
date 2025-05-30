// End-to-End Test Modules
// Complete user workflow and cross-service integration testing

// Basic Integration Tests
pub mod integration_test_basic;

// Session Management E2E Tests
pub mod webhook_session_management_test;

// User Journey Tests - Complete user workflows from start to finish
pub mod user_journey_e2e_test;

// Service Integration Tests - Cross-service data flow and interaction testing
pub mod service_integration_e2e_test;

// RBAC Comprehensive Tests - Role-based access control validation
pub mod rbac_comprehensive_user_journey_test;

// Invitation System Tests - Complete invitation flow testing
// Disabled test moved to tests/disabled/
// pub mod invitation_system_e2e_test;

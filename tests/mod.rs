// Integration Test Modules
// Service-level integration and component interaction testing

// Common test utilities
pub mod common;

// Integration test modules
pub mod integration {
    pub mod data_integrity_test;
    pub mod monitoring_reliability_integration_test;
    pub mod persistence_layer_integration_test;
    pub mod service_communication_test;
    pub mod session_opportunity_integration_test;
}

// E2E test modules
pub mod e2e {
    pub mod integration_test_basic;
    pub mod webhook_session_management_test;
}

// Unit test modules - currently empty as we removed outdated tests
pub mod unit {
    // Unit tests will be organized by module when needed
}

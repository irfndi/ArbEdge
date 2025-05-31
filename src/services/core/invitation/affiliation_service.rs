use crate::services::core::infrastructure::database_repositories::DatabaseManager;
use crate::utils::{ArbitrageError, ArbitrageResult};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// Constants for affiliation calculations
const DEFAULT_AVERAGE_SUBSCRIPTION_COST: f64 = 29.0; // $29 average subscription
const DEFAULT_COMMISSION_RATE: f64 = 0.1; // 10% commission rate

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffiliationConfig {
    pub volume_thresholds: Vec<(u32, f64)>, // (referral_count, bonus)
    pub quality_thresholds: Vec<(f64, f64)>, // (conversion_rate, bonus)
    pub tier_thresholds: Vec<(f64, f64, PerformanceTier)>, // (revenue, engagement, tier)
}

impl Default for AffiliationConfig {
    fn default() -> Self {
        Self {
            volume_thresholds: vec![
                (50, 10.0), // High volume bonus
                (20, 5.0),  // Medium volume bonus
                (10, 2.0),  // Low volume bonus
            ],
            quality_thresholds: vec![
                (50.0, 15.0), // Exceptional conversion rate
                (30.0, 10.0), // High conversion rate
                (20.0, 5.0),  // Good conversion rate
            ],
            tier_thresholds: vec![
                (10000.0, 80.0, PerformanceTier::Diamond), // Top tier
                (5000.0, 70.0, PerformanceTier::Platinum), // High tier
                (2000.0, 60.0, PerformanceTier::Gold),     // Mid-high tier
                (500.0, 40.0, PerformanceTier::Silver),    // Mid tier
                                                           // Bronze is default for below thresholds
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffiliationProgram {
    pub id: String,
    pub user_id: String,
    pub program_type: AffiliationProgramType,
    pub verification_status: VerificationStatus,
    pub follower_count: Option<u32>,
    pub platform: Option<String>, // "twitter", "youtube", "telegram", etc.
    pub kickback_rate: f64,       // Percentage of revenue sharing
    pub special_features: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub verified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AffiliationProgramType {
    Influencer,
    ContentCreator,
    TradingEducator,
    CommunityLeader,
    TechnicalAnalyst,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pending,
    UnderReview,
    Approved,
    Rejected,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffiliationApplication {
    pub id: String,
    pub user_id: String,
    pub program_type: AffiliationProgramType,
    pub platform: String,
    pub follower_count: u32,
    pub content_examples: Vec<String>,
    pub trading_experience: String,
    pub motivation: String,
    pub status: VerificationStatus,
    pub created_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<String>,
    pub review_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffiliationMetrics {
    pub user_id: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub referrals_generated: u32,
    pub conversions: u32,
    pub revenue_generated: f64,
    pub kickback_earned: f64,
    pub engagement_score: f64,
    pub performance_tier: PerformanceTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceTier {
    Bronze,
    Silver,
    Gold,
    Platinum,
    Diamond,
}

pub struct AffiliationService {
    d1_service: DatabaseManager,
    config: AffiliationConfig,
}

impl AffiliationService {
    pub fn new(d1_service: DatabaseManager) -> Self {
        Self {
            d1_service,
            config: AffiliationConfig::default(),
        }
    }

    pub fn new_with_config(d1_service: DatabaseManager, config: AffiliationConfig) -> Self {
        Self { d1_service, config }
    }

    /// Submit an application for affiliation program
    pub async fn submit_application(
        &self,
        application: AffiliationApplication,
    ) -> ArbitrageResult<AffiliationApplication> {
        // Validate application
        self.validate_application(&application)?;

        // Check if user already has an active application
        if self.has_pending_application(&application.user_id).await? {
            return Err(ArbitrageError::validation_error(
                "User already has a pending affiliation application",
            ));
        }

        // Store the application
        self.store_application(&application).await?;

        Ok(application)
    }

    /// Review an affiliation application (Admin only)
    pub async fn review_application(
        &self,
        application_id: &str,
        reviewer_id: &str,
        decision: VerificationStatus,
        notes: Option<String>,
    ) -> ArbitrageResult<AffiliationApplication> {
        // Verify reviewer has admin permissions
        if !self.verify_admin_permission(reviewer_id).await? {
            return Err(ArbitrageError::unauthorized(
                "Only admins can review affiliation applications",
            ));
        }

        // Update application status
        let query = r#"
            UPDATE affiliation_applications 
            SET status = ?, reviewed_at = ?, reviewed_by = ?, review_notes = ?
            WHERE id = ?
        "#;

        self.d1_service
            .execute(
                query,
                &[
                    format!("{:?}", decision).into(),
                    Utc::now().to_rfc3339().into(),
                    reviewer_id.into(),
                    notes.unwrap_or_default().into(),
                    application_id.into(),
                ],
            )
            .await?;

        // If approved, create affiliation program
        if matches!(decision, VerificationStatus::Approved) {
            self.create_affiliation_program_from_application(application_id, reviewer_id)
                .await?;
        }

        // Return updated application
        self.get_application_by_id(application_id).await
    }

    /// Get affiliation program for a user
    pub async fn get_user_affiliation(
        &self,
        user_id: &str,
    ) -> ArbitrageResult<Option<AffiliationProgram>> {
        let query = r#"
            SELECT id, user_id, program_type, verification_status, follower_count, platform, 
                   kickback_rate, special_features, created_at, updated_at, verified_at, verified_by
            FROM affiliation_programs 
            WHERE user_id = ? AND verification_status = 'Approved'
        "#;

        let result = self.d1_service.query(query, &[user_id.into()]).await?;

        let results_vec = result.results::<HashMap<String, serde_json::Value>>()?;
        if let Some(row) = results_vec.first() {
            Ok(Some(self.parse_affiliation_program_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    /// Calculate affiliation metrics for a user
    pub async fn calculate_metrics(
        &self,
        user_id: &str,
        period_days: u32,
    ) -> ArbitrageResult<AffiliationMetrics> {
        let period_start = Utc::now() - chrono::Duration::days(period_days as i64);
        let period_end = Utc::now();

        let referrals_generated = self
            .count_referrals_in_period(user_id, &period_start, &period_end)
            .await?;
        let conversions = self
            .count_conversions_in_period(user_id, &period_start, &period_end)
            .await?;
        let revenue_generated = self
            .calculate_revenue_in_period(user_id, &period_start, &period_end)
            .await?;

        // Get kickback rate for user
        let kickback_rate = if let Some(program) = self.get_user_affiliation(user_id).await? {
            program.kickback_rate
        } else {
            0.0
        };

        let kickback_earned = revenue_generated * (kickback_rate / 100.0);
        let engagement_score = self
            .calculate_engagement_score(referrals_generated, conversions)
            .await?;
        let performance_tier = self
            .determine_performance_tier(engagement_score, revenue_generated)
            .await?;

        Ok(AffiliationMetrics {
            user_id: user_id.to_string(),
            period_start,
            period_end,
            referrals_generated,
            conversions,
            revenue_generated,
            kickback_earned,
            engagement_score,
            performance_tier,
        })
    }

    /// Get all pending applications for admin review
    pub async fn get_pending_applications(
        &self,
        reviewer_id: &str,
    ) -> ArbitrageResult<Vec<AffiliationApplication>> {
        if !self.verify_admin_permission(reviewer_id).await? {
            return Err(ArbitrageError::unauthorized(
                "Only admins can view pending applications",
            ));
        }

        let query = r#"
            SELECT id, user_id, program_type, platform, follower_count, content_examples, 
                   trading_experience, motivation, status, created_at, reviewed_at, reviewed_by, review_notes
            FROM affiliation_applications 
            WHERE status IN ('Pending', 'UnderReview')
            ORDER BY created_at ASC
        "#;

        let result = self.d1_service.query(query, &[]).await?;

        let mut applications = Vec::new();
        let results_vec = result.results::<HashMap<String, serde_json::Value>>()?;
        for row in results_vec {
            applications.push(self.parse_application_from_row(&row)?);
        }

        Ok(applications)
    }

    /// Update affiliation program settings
    pub async fn update_program_settings(
        &self,
        user_id: &str,
        kickback_rate: Option<f64>,
        special_features: Option<Vec<String>>,
    ) -> ArbitrageResult<AffiliationProgram> {
        let mut updates = Vec::new();
        let mut params = Vec::new();

        if let Some(rate) = kickback_rate {
            // Validate kickback rate is within acceptable range (0-100%)
            if !(0.0..=100.0).contains(&rate) {
                return Err(ArbitrageError::validation_error(format!(
                    "Kickback rate must be between 0 and 100 percent, got: {}",
                    rate
                )));
            }
            updates.push("kickback_rate = ?");
            params.push(rate.into());
        }

        if let Some(features) = special_features {
            updates.push("special_features = ?");
            let serialized_features = serde_json::to_string(&features).map_err(|e| {
                ArbitrageError::parse_error(format!("Failed to serialize special_features: {}", e))
            })?;
            params.push(serialized_features.into());
        }

        if updates.is_empty() {
            return Err(ArbitrageError::validation_error("No updates provided"));
        }

        updates.push("updated_at = ?");
        params.push(Utc::now().to_rfc3339().into());
        params.push(user_id.into());

        let query = format!(
            "UPDATE affiliation_programs SET {} WHERE user_id = ?",
            updates.join(", ")
        );

        self.d1_service.execute(&query, &params).await?;

        // Return updated program
        self.get_user_affiliation(user_id)
            .await?
            .ok_or_else(|| ArbitrageError::not_found("Affiliation program not found"))
    }

    /// Get top performing affiliates
    pub async fn get_top_performers(&self, limit: u32) -> ArbitrageResult<Vec<AffiliationMetrics>> {
        // Get all active affiliation programs
        let query = r#"
            SELECT ap.user_id, ap.created_at
            FROM affiliation_programs ap
            WHERE ap.verification_status = 'Approved'
            ORDER BY ap.created_at ASC
            LIMIT ?
        "#;

        let result = self
            .d1_service
            .query(query, &[limit.to_string().into()])
            .await?;

        // Collect user IDs for concurrent processing
        let user_ids: Vec<String> = result
            .results::<HashMap<String, serde_json::Value>>()?
            .iter()
            .filter_map(|row| row.get("user_id").map(|id| id.to_string()))
            .collect();

        // Calculate metrics concurrently for all users
        let metrics_futures = user_ids
            .iter()
            .map(|user_id| self.calculate_metrics(user_id, 30));

        let metrics_results = join_all(metrics_futures).await;

        // Filter successful results and active performers
        let mut top_performers = Vec::new();
        for (i, result) in metrics_results.into_iter().enumerate() {
            match result {
                Ok(metrics) => {
                    // Only include performers with some activity
                    if metrics.referrals_generated > 0 || metrics.revenue_generated > 0.0 {
                        top_performers.push(metrics);
                    }
                }
                Err(e) => {
                    if user_ids.get(i).is_some() {
                        // Log without user identifier to avoid security scanner warnings
                        crate::utils::logger::logger()
                            .warn(&format!("Failed to calculate metrics for affiliate: {}", e));
                    }
                }
            }
        }

        // Sort by engagement score (descending) then by revenue (descending)
        top_performers.sort_by(|a, b| {
            b.engagement_score
                .partial_cmp(&a.engagement_score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.revenue_generated
                        .partial_cmp(&a.revenue_generated)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });

        Ok(top_performers)
    }

    // Private helper methods

    fn validate_application(&self, application: &AffiliationApplication) -> ArbitrageResult<()> {
        if application.follower_count < 1000 {
            return Err(ArbitrageError::validation_error(
                "Minimum follower count requirement not met (1000+)",
            ));
        }

        if application.content_examples.is_empty() {
            return Err(ArbitrageError::validation_error(
                "At least one content example is required",
            ));
        }

        if application.motivation.len() < 50 {
            return Err(ArbitrageError::validation_error(
                "Motivation statement must be at least 50 characters",
            ));
        }

        Ok(())
    }

    async fn has_pending_application(&self, user_id: &str) -> ArbitrageResult<bool> {
        let query = r#"
            SELECT COUNT(*) as count 
            FROM affiliation_applications 
            WHERE user_id = ? AND status IN ('Pending', 'UnderReview')
        "#;

        let result = self.d1_service.query(query, &[user_id.into()]).await?;

        let results_vec = result.results::<HashMap<String, serde_json::Value>>()?;
        if let Some(row) = results_vec.first() {
            let count_str = row.get("count").and_then(|v| v.as_str()).unwrap_or("0");
            let count: i32 = count_str.parse().unwrap_or(0);
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    async fn verify_admin_permission(&self, user_id: &str) -> ArbitrageResult<bool> {
        // Query user profile to check if they have super admin role
        let query =
            "SELECT profile_metadata, subscription_tier FROM user_profiles WHERE user_id = ?";
        let result = self.d1_service.query(query, &[user_id.into()]).await?;

        let results_vec = result.results::<HashMap<String, serde_json::Value>>()?;
        if let Some(row) = results_vec.first() {
            // Check subscription tier
            if let Some(tier_str) = row.get("subscription_tier").and_then(|v| v.as_str()) {
                if tier_str == "SuperAdmin" {
                    return Ok(true);
                }
            }

            // Check role in metadata
            if let Some(metadata_str) = row.get("profile_metadata").and_then(|v| v.as_str()) {
                if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_str) {
                    if let Some(role) = metadata.get("role") {
                        if role == "SuperAdmin" {
                            return Ok(true);
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    async fn store_application(&self, application: &AffiliationApplication) -> ArbitrageResult<()> {
        let query = r#"
            INSERT INTO affiliation_applications 
            (id, user_id, program_type, platform, follower_count, content_examples, 
             trading_experience, motivation, status, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        self.d1_service
            .execute(
                query,
                &[
                    application.id.clone().into(),
                    application.user_id.clone().into(),
                    format!("{:?}", application.program_type).into(),
                    application.platform.clone().into(),
                    application.follower_count.into(),
                    serde_json::to_string(&application.content_examples)
                        .map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Failed to serialize content_examples: {}",
                                e
                            ))
                        })?
                        .into(),
                    application.trading_experience.clone().into(),
                    application.motivation.clone().into(),
                    format!("{:?}", application.status).into(),
                    application.created_at.to_rfc3339().into(),
                ],
            )
            .await?;

        Ok(())
    }

    async fn create_affiliation_program_from_application(
        &self,
        application_id: &str,
        reviewer_id: &str,
    ) -> ArbitrageResult<()> {
        // Get application details
        let application = self.get_application_by_id(application_id).await?;

        // Determine kickback rate based on program type and follower count
        let kickback_rate = Self::calculate_initial_kickback_rate(
            &application.program_type,
            application.follower_count,
        );

        let program_type = application.program_type.clone();
        let program = AffiliationProgram {
            id: Uuid::new_v4().to_string(),
            user_id: application.user_id,
            program_type: program_type.clone(),
            verification_status: VerificationStatus::Approved,
            follower_count: Some(application.follower_count),
            platform: Some(application.platform),
            kickback_rate,
            special_features: Self::get_default_special_features(&program_type),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            verified_at: Some(Utc::now()),
            verified_by: Some(reviewer_id.to_string()),
        };

        self.store_affiliation_program(&program).await?;
        Ok(())
    }

    fn calculate_initial_kickback_rate(
        program_type: &AffiliationProgramType,
        follower_count: u32,
    ) -> f64 {
        let base_rate = match program_type {
            AffiliationProgramType::Influencer => 15.0,
            AffiliationProgramType::ContentCreator => 12.0,
            AffiliationProgramType::TradingEducator => 20.0,
            AffiliationProgramType::CommunityLeader => 10.0,
            AffiliationProgramType::TechnicalAnalyst => 18.0,
            AffiliationProgramType::Enterprise => 25.0,
        };

        // Bonus based on follower count
        let follower_bonus = match follower_count {
            0..=10000 => 0.0,
            10001..=50000 => 2.0,
            50001..=100000 => 5.0,
            100001..=500000 => 8.0,
            _ => 10.0,
        };

        base_rate + follower_bonus
    }

    fn get_default_special_features(program_type: &AffiliationProgramType) -> Vec<String> {
        match program_type {
            AffiliationProgramType::Influencer => vec![
                "priority_support".to_string(),
                "custom_referral_links".to_string(),
                "monthly_analytics".to_string(),
            ],
            AffiliationProgramType::TradingEducator => vec![
                "educational_content_access".to_string(),
                "webinar_hosting".to_string(),
                "student_discount_codes".to_string(),
            ],
            AffiliationProgramType::Enterprise => vec![
                "white_label_options".to_string(),
                "api_access".to_string(),
                "dedicated_account_manager".to_string(),
            ],
            _ => vec![
                "priority_support".to_string(),
                "monthly_analytics".to_string(),
            ],
        }
    }

    async fn store_affiliation_program(&self, program: &AffiliationProgram) -> ArbitrageResult<()> {
        let query = r#"
            INSERT INTO affiliation_programs 
            (id, user_id, program_type, verification_status, follower_count, platform, 
             kickback_rate, special_features, created_at, updated_at, verified_at, verified_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#;

        self.d1_service
            .execute(
                query,
                &[
                    program.id.clone().into(),
                    program.user_id.clone().into(),
                    format!("{:?}", program.program_type).into(),
                    format!("{:?}", program.verification_status).into(),
                    program
                        .follower_count
                        .map(worker::wasm_bindgen::JsValue::from)
                        .unwrap_or_else(|| worker::wasm_bindgen::JsValue::NULL),
                    program.platform.clone().unwrap_or_default().into(),
                    program.kickback_rate.into(),
                    serde_json::to_string(&program.special_features)
                        .map_err(|e| {
                            ArbitrageError::parse_error(format!(
                                "Failed to serialize special_features: {}",
                                e
                            ))
                        })?
                        .into(),
                    program.created_at.to_rfc3339().into(),
                    program.updated_at.to_rfc3339().into(),
                    program
                        .verified_at
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                        .into(),
                    program.verified_by.clone().unwrap_or_default().into(),
                ],
            )
            .await?;

        Ok(())
    }

    async fn get_application_by_id(
        &self,
        application_id: &str,
    ) -> ArbitrageResult<AffiliationApplication> {
        let query = r#"
            SELECT id, user_id, program_type, platform, follower_count, content_examples, 
                   trading_experience, motivation, status, created_at, reviewed_at, reviewed_by, review_notes
            FROM affiliation_applications 
            WHERE id = ?
        "#;

        let result = self
            .d1_service
            .query(query, &[application_id.into()])
            .await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            self.parse_application_from_row(row)
        } else {
            Err(ArbitrageError::not_found("Application not found"))
        }
    }

    fn parse_application_from_row(
        &self,
        row: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<AffiliationApplication> {
        Ok(AffiliationApplication {
            id: row
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing required field: id"))?
                .to_string(),
            user_id: row
                .get("user_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing required field: user_id"))?
                .to_string(),
            program_type: self.parse_program_type(
                row.get("program_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing required field: program_type")
                    })?,
            )?,
            platform: row
                .get("platform")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing required field: platform"))?
                .to_string(),
            follower_count: row
                .get("follower_count")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32)
                .ok_or_else(|| ArbitrageError::parse_error("Missing follower_count field"))?,
            content_examples: row
                .get("content_examples")
                .and_then(|v| v.as_str())
                .map(|s| serde_json::from_str(s).unwrap_or_default())
                .unwrap_or_default(),
            trading_experience: row
                .get("trading_experience")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ArbitrageError::parse_error("Missing required field: trading_experience")
                })?
                .to_string(),
            motivation: row
                .get("motivation")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing required field: motivation"))?
                .to_string(),
            status: self.parse_verification_status(
                row.get("status")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ArbitrageError::parse_error("Missing required field: status"))?,
            )?,
            created_at: row
                .get("created_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .ok_or_else(|| {
                    ArbitrageError::parse_error("Missing or invalid created_at field")
                })?,
            reviewed_at: row
                .get("reviewed_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            reviewed_by: row
                .get("reviewed_by")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            review_notes: row
                .get("review_notes")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    fn parse_affiliation_program_from_row(
        &self,
        row: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<AffiliationProgram> {
        Ok(AffiliationProgram {
            id: row
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing required field: id"))?
                .to_string(),
            user_id: row
                .get("user_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ArbitrageError::parse_error("Missing required field: user_id"))?
                .to_string(),
            program_type: self.parse_program_type(
                row.get("program_type")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing required field: program_type")
                    })?,
            )?,
            verification_status: self.parse_verification_status(
                row.get("verification_status")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ArbitrageError::parse_error("Missing required field: verification_status")
                    })?,
            )?,
            follower_count: row
                .get("follower_count")
                .and_then(|v| v.as_u64())
                .map(|v| v as u32),
            platform: row
                .get("platform")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            kickback_rate: row
                .get("kickback_rate")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| ArbitrageError::parse_error("Missing kickback_rate field"))?,
            special_features: row
                .get("special_features")
                .and_then(|v| v.as_str())
                .map(|s| serde_json::from_str(s).unwrap_or_default())
                .unwrap_or_default(),
            created_at: row
                .get("created_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .ok_or_else(|| {
                    ArbitrageError::parse_error("Missing or invalid created_at field")
                })?,
            updated_at: row
                .get("updated_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .ok_or_else(|| {
                    ArbitrageError::parse_error("Missing or invalid updated_at field")
                })?,
            verified_at: row
                .get("verified_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            verified_by: row
                .get("verified_by")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
    }

    fn parse_program_type(&self, type_str: &str) -> ArbitrageResult<AffiliationProgramType> {
        match type_str {
            "Influencer" => Ok(AffiliationProgramType::Influencer),
            "ContentCreator" => Ok(AffiliationProgramType::ContentCreator),
            "TradingEducator" => Ok(AffiliationProgramType::TradingEducator),
            "CommunityLeader" => Ok(AffiliationProgramType::CommunityLeader),
            "TechnicalAnalyst" => Ok(AffiliationProgramType::TechnicalAnalyst),
            "Enterprise" => Ok(AffiliationProgramType::Enterprise),
            _ => Err(ArbitrageError::parse_error(format!(
                "Invalid program type: {}",
                type_str
            ))),
        }
    }

    fn parse_verification_status(&self, status_str: &str) -> ArbitrageResult<VerificationStatus> {
        match status_str {
            "Pending" => Ok(VerificationStatus::Pending),
            "UnderReview" => Ok(VerificationStatus::UnderReview),
            "Approved" => Ok(VerificationStatus::Approved),
            "Rejected" => Ok(VerificationStatus::Rejected),
            "Suspended" => Ok(VerificationStatus::Suspended),
            _ => Err(ArbitrageError::parse_error(format!(
                "Invalid verification status: {}",
                status_str
            ))),
        }
    }

    // Real metrics calculation methods
    async fn count_referrals_in_period(
        &self,
        user_id: &str,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> ArbitrageResult<u32> {
        let query = r#"
            SELECT COUNT(*) as count 
            FROM referral_usage ru
            JOIN user_referral_codes urc ON ru.referrer_user_id = urc.user_id
            WHERE urc.user_id = ? AND ru.used_at >= ? AND ru.used_at <= ?
        "#;

        let result = self
            .d1_service
            .query(
                query,
                &[
                    user_id.into(),
                    start.to_rfc3339().into(),
                    end.to_rfc3339().into(),
                ],
            )
            .await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            let count_str = row.get("count").and_then(|v| v.as_str()).unwrap_or("0");
            let count: i32 = count_str.parse().unwrap_or(0);
            Ok(count as u32)
        } else {
            Ok(0)
        }
    }

    async fn count_conversions_in_period(
        &self,
        user_id: &str,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> ArbitrageResult<u32> {
        let query = r#"
            SELECT COUNT(*) as count 
            FROM referral_usage ru
            JOIN user_trading_preferences utp ON ru.referred_user_id = utp.user_id
            WHERE ru.referrer_user_id = ? 
            AND ru.used_at >= ? AND ru.used_at <= ?
            AND ru.conversion_status IN ('Subscribed', 'ActiveUser', 'FirstTrade')
            AND utp.created_at >= ru.used_at
        "#;

        let result = self
            .d1_service
            .query(
                query,
                &[
                    user_id.into(),
                    start.to_rfc3339().into(),
                    end.to_rfc3339().into(),
                ],
            )
            .await?;

        if let Some(row) = result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            let count_str = row.get("count").and_then(|v| v.as_str()).unwrap_or("0");
            let count: i32 = count_str.parse().unwrap_or(0);
            Ok(count as u32)
        } else {
            Ok(0)
        }
    }

    async fn calculate_revenue_in_period(
        &self,
        user_id: &str,
        start: &DateTime<Utc>,
        end: &DateTime<Utc>,
    ) -> ArbitrageResult<f64> {
        // Calculate revenue from referral bonuses and subscription fees
        let bonus_query = r#"
            SELECT COALESCE(SUM(ru.bonus_awarded), 0) as total_bonuses
            FROM referral_usage ru
            WHERE ru.referrer_user_id = ? 
            AND ru.used_at >= ? AND ru.used_at <= ?
        "#;

        let bonus_result = self
            .d1_service
            .query(
                bonus_query,
                &[
                    user_id.into(),
                    start.to_rfc3339().into(),
                    end.to_rfc3339().into(),
                ],
            )
            .await?;

        let total_bonuses = if let Some(row) = bonus_result
            .results::<HashMap<String, serde_json::Value>>()?
            .first()
        {
            let bonus_str = row
                .get("total_bonuses")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0");
            bonus_str.parse().unwrap_or(0.0)
        } else {
            0.0
        };

        // Calculate commission from referred users' subscription fees (estimated)
        let conversion_count = self
            .count_conversions_in_period(user_id, start, end)
            .await?;
        let estimated_subscription_revenue =
            conversion_count as f64 * DEFAULT_AVERAGE_SUBSCRIPTION_COST;
        let subscription_commission = estimated_subscription_revenue * DEFAULT_COMMISSION_RATE;

        Ok(total_bonuses + subscription_commission)
    }

    async fn calculate_engagement_score(
        &self,
        referrals: u32,
        conversions: u32,
    ) -> ArbitrageResult<f64> {
        if referrals == 0 {
            return Ok(0.0);
        }

        // Base conversion rate (0-100)
        let conversion_rate = (conversions as f64 / referrals as f64) * 100.0;

        // Bonus factors for high activity (configurable)
        let volume_bonus = self
            .config
            .volume_thresholds
            .iter()
            .find(|(threshold, _)| referrals >= *threshold)
            .map(|(_, bonus)| *bonus)
            .unwrap_or(0.0);

        // Quality bonus for high conversion rates (configurable)
        let quality_bonus = self
            .config
            .quality_thresholds
            .iter()
            .find(|(threshold, _)| conversion_rate >= *threshold)
            .map(|(_, bonus)| *bonus)
            .unwrap_or(0.0);

        // Calculate final engagement score (capped at 100)
        let engagement_score = (conversion_rate + volume_bonus + quality_bonus).min(100.0);

        Ok(engagement_score)
    }

    async fn determine_performance_tier(
        &self,
        engagement_score: f64,
        revenue: f64,
    ) -> ArbitrageResult<PerformanceTier> {
        // Tier determination based on both engagement score and revenue (configurable)
        let tier = self
            .config
            .tier_thresholds
            .iter()
            .find(|(revenue_threshold, engagement_threshold, _)| {
                revenue >= *revenue_threshold && engagement_score >= *engagement_threshold
            })
            .map(|(_, _, tier)| tier.clone())
            .unwrap_or(PerformanceTier::Bronze); // Default to Bronze if no thresholds met

        Ok(tier)
    }
}

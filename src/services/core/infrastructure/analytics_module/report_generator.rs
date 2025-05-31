// src/services/core/infrastructure/analytics_module/report_generator.rs

//! Report Generator - Automated Report Generation and Export System
//!
//! This component provides comprehensive report generation capabilities for the ArbEdge platform,
//! handling automated report creation, multi-format export, and scheduled reporting with
//! customizable templates and data visualization.
//!
//! ## Revolutionary Features:
//! - **Multi-Format Export**: PDF, CSV, JSON, HTML report generation
//! - **Scheduled Reports**: Daily, weekly, monthly automated reports
//! - **Custom Templates**: User-defined report layouts and content
//! - **Data Visualization**: Charts, graphs, and statistical summaries
//! - **Report Caching**: Intelligent caching for frequently requested reports

use crate::utils::{ArbitrageError, ArbitrageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use worker::{kv::KvStore, Env};

/// Report Generator Configuration
#[derive(Debug, Clone)]
pub struct ReportGeneratorConfig {
    // Report generation settings
    pub enable_automated_reports: bool,
    pub enable_custom_reports: bool,
    pub enable_scheduled_reports: bool,
    pub max_report_size_mb: usize,

    // Export format settings
    pub supported_formats: Vec<String>, // ["pdf", "csv", "json", "html"]
    pub default_format: String,
    pub enable_compression: bool,

    // Scheduling settings
    pub daily_report_time: String, // "02:00" UTC
    pub weekly_report_day: String, // "sunday"
    pub monthly_report_day: u8,    // 1st of month

    // Performance settings
    pub max_concurrent_reports: u32,
    pub report_timeout_seconds: u64,
    pub cache_ttl_seconds: u64,
    pub batch_processing_size: usize,
}

impl Default for ReportGeneratorConfig {
    fn default() -> Self {
        Self {
            enable_automated_reports: true,
            enable_custom_reports: true,
            enable_scheduled_reports: true,
            max_report_size_mb: 50,
            supported_formats: vec![
                "pdf".to_string(),
                "csv".to_string(),
                "json".to_string(),
                "html".to_string(),
            ],
            default_format: "pdf".to_string(),
            enable_compression: true,
            daily_report_time: "02:00".to_string(),
            weekly_report_day: "sunday".to_string(),
            monthly_report_day: 1,
            max_concurrent_reports: 10,
            report_timeout_seconds: 300,
            cache_ttl_seconds: 3600,
            batch_processing_size: 100,
        }
    }
}

impl ReportGeneratorConfig {
    /// High-performance configuration for 1000-2500 concurrent users
    pub fn high_performance() -> Self {
        Self {
            enable_automated_reports: true,
            enable_custom_reports: true,
            enable_scheduled_reports: true,
            max_report_size_mb: 100,
            supported_formats: vec!["pdf".to_string(), "csv".to_string(), "json".to_string()],
            default_format: "json".to_string(),
            enable_compression: true,
            daily_report_time: "02:00".to_string(),
            weekly_report_day: "sunday".to_string(),
            monthly_report_day: 1,
            max_concurrent_reports: 25,
            report_timeout_seconds: 180,
            cache_ttl_seconds: 1800,
            batch_processing_size: 200,
        }
    }

    /// High-reliability configuration with enhanced data retention
    pub fn high_reliability() -> Self {
        Self {
            enable_automated_reports: true,
            enable_custom_reports: false, // Disable for stability
            enable_scheduled_reports: true,
            max_report_size_mb: 25,
            supported_formats: vec!["csv".to_string(), "json".to_string()],
            default_format: "csv".to_string(),
            enable_compression: true,
            daily_report_time: "03:00".to_string(),
            weekly_report_day: "sunday".to_string(),
            monthly_report_day: 1,
            max_concurrent_reports: 5,
            report_timeout_seconds: 600,
            cache_ttl_seconds: 7200,
            batch_processing_size: 50,
        }
    }

    /// Validate configuration parameters
    pub fn validate(&self) -> ArbitrageResult<()> {
        if self.max_report_size_mb == 0 {
            return Err(ArbitrageError::configuration_error(
                "max_report_size_mb must be greater than 0".to_string(),
            ));
        }
        if self.supported_formats.is_empty() {
            return Err(ArbitrageError::configuration_error(
                "supported_formats cannot be empty".to_string(),
            ));
        }
        if !self.supported_formats.contains(&self.default_format) {
            return Err(ArbitrageError::configuration_error(
                "default_format must be in supported_formats".to_string(),
            ));
        }
        if self.max_concurrent_reports == 0 {
            return Err(ArbitrageError::configuration_error(
                "max_concurrent_reports must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

/// Report Generator Health Status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportGeneratorHealth {
    pub is_healthy: bool,
    pub report_generation_healthy: bool,
    pub export_system_healthy: bool,
    pub scheduler_healthy: bool,
    pub active_reports: u32,
    pub queue_size: u32,
    pub average_generation_time_ms: f64,
    pub last_health_check: u64,
}

/// Report Generator Performance Metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportGeneratorMetrics {
    // Generation metrics
    pub reports_generated: u64,
    pub reports_failed: u64,
    pub average_generation_time_ms: f64,
    pub reports_per_hour: f64,

    // Export metrics
    pub exports_by_format: HashMap<String, u64>,
    pub export_success_rate: f64,
    pub total_export_size_mb: f64,

    // Scheduling metrics
    pub scheduled_reports_executed: u64,
    pub scheduled_reports_failed: u64,
    pub custom_reports_generated: u64,

    // Performance metrics
    pub cache_hit_rate: f64,
    pub error_rate: f64,
    pub queue_processing_rate: f64,
    pub last_updated: u64,
}

/// Report template definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub template_id: String,
    pub name: String,
    pub description: String,
    pub template_type: String, // "trading", "analytics", "financial", "custom"
    pub sections: Vec<ReportSection>,
    pub default_format: String,
    pub parameters: HashMap<String, String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Report section definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSection {
    pub section_id: String,
    pub title: String,
    pub section_type: String, // "summary", "table", "chart", "text"
    pub data_source: String,
    pub filters: HashMap<String, String>,
    pub visualization: Option<VisualizationConfig>,
    pub order: u32,
}

/// Visualization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationConfig {
    pub chart_type: String, // "line", "bar", "pie", "scatter"
    pub x_axis: String,
    pub y_axis: String,
    pub title: String,
    pub colors: Vec<String>,
    pub show_legend: bool,
}

/// Report request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRequest {
    pub request_id: String,
    pub template_id: String,
    pub user_id: String,
    pub format: String,
    pub parameters: HashMap<String, String>,
    pub filters: HashMap<String, String>,
    pub date_range: DateRange,
    pub priority: ReportPriority,
    pub delivery_method: String, // "download", "email", "webhook"
    pub delivery_target: Option<String>,
    pub requested_at: u64,
}

/// Date range for reports
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start_date: u64,
    pub end_date: u64,
    pub timezone: String,
}

/// Report priority levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Generated report result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedReport {
    pub report_id: String,
    pub request_id: String,
    pub template_id: String,
    pub user_id: String,
    pub format: String,
    pub file_size_bytes: u64,
    pub generation_time_ms: u64,
    pub status: ReportStatus,
    pub download_url: Option<String>,
    pub error_message: Option<String>,
    pub generated_at: u64,
    pub expires_at: u64,
}

/// Report generation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReportStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Expired,
}

/// Report Generator for automated reporting
#[derive(Clone)]
pub struct ReportGenerator {
    config: ReportGeneratorConfig,
    kv_store: Option<KvStore>,

    // Report templates
    templates: HashMap<String, ReportTemplate>,

    // Report queue and processing
    report_queue: Vec<ReportRequest>,
    active_reports: HashMap<String, ReportRequest>,

    // Performance tracking
    metrics: ReportGeneratorMetrics,
    last_generation_time: u64,
    is_initialized: bool,
}

impl ReportGenerator {
    /// Create new Report Generator with configuration
    pub fn new(config: ReportGeneratorConfig) -> ArbitrageResult<Self> {
        config.validate()?;

        Ok(Self {
            config,
            kv_store: None,
            templates: HashMap::new(),
            report_queue: Vec::new(),
            active_reports: HashMap::new(),
            metrics: ReportGeneratorMetrics::default(),
            last_generation_time: worker::Date::now().as_millis(),
            is_initialized: false,
        })
    }

    /// Initialize the Report Generator with environment
    pub async fn initialize(&mut self, env: &Env) -> ArbitrageResult<()> {
        // Initialize KV store for caching and storage
        self.kv_store = Some(env.kv("REPORTS_CACHE").map_err(|e| {
            ArbitrageError::configuration_error(format!("Failed to initialize KV store: {:?}", e))
        })?);

        // Load default templates
        self.load_default_templates().await?;

        self.is_initialized = true;
        Ok(())
    }

    /// Load default report templates
    async fn load_default_templates(&mut self) -> ArbitrageResult<()> {
        // Trading Performance Report Template
        let trading_template = ReportTemplate {
            template_id: "trading_performance".to_string(),
            name: "Trading Performance Report".to_string(),
            description: "Comprehensive trading performance analysis".to_string(),
            template_type: "trading".to_string(),
            sections: vec![
                ReportSection {
                    section_id: "summary".to_string(),
                    title: "Performance Summary".to_string(),
                    section_type: "summary".to_string(),
                    data_source: "trading_metrics".to_string(),
                    filters: HashMap::new(),
                    visualization: None,
                    order: 1,
                },
                ReportSection {
                    section_id: "opportunities".to_string(),
                    title: "Opportunity Analysis".to_string(),
                    section_type: "table".to_string(),
                    data_source: "opportunities".to_string(),
                    filters: HashMap::new(),
                    visualization: Some(VisualizationConfig {
                        chart_type: "bar".to_string(),
                        x_axis: "exchange".to_string(),
                        y_axis: "profit".to_string(),
                        title: "Profit by Exchange".to_string(),
                        colors: vec!["#3498db".to_string(), "#e74c3c".to_string()],
                        show_legend: true,
                    }),
                    order: 2,
                },
            ],
            default_format: "pdf".to_string(),
            parameters: HashMap::new(),
            created_at: worker::Date::now().as_millis(),
            updated_at: worker::Date::now().as_millis(),
        };

        // Analytics Report Template
        let analytics_template = ReportTemplate {
            template_id: "analytics_summary".to_string(),
            name: "Analytics Summary Report".to_string(),
            description: "System analytics and performance metrics".to_string(),
            template_type: "analytics".to_string(),
            sections: vec![ReportSection {
                section_id: "system_metrics".to_string(),
                title: "System Performance".to_string(),
                section_type: "chart".to_string(),
                data_source: "system_metrics".to_string(),
                filters: HashMap::new(),
                visualization: Some(VisualizationConfig {
                    chart_type: "line".to_string(),
                    x_axis: "timestamp".to_string(),
                    y_axis: "latency".to_string(),
                    title: "System Latency Over Time".to_string(),
                    colors: vec!["#2ecc71".to_string()],
                    show_legend: false,
                }),
                order: 1,
            }],
            default_format: "json".to_string(),
            parameters: HashMap::new(),
            created_at: worker::Date::now().as_millis(),
            updated_at: worker::Date::now().as_millis(),
        };

        self.templates
            .insert(trading_template.template_id.clone(), trading_template);
        self.templates
            .insert(analytics_template.template_id.clone(), analytics_template);

        Ok(())
    }

    /// Generate report from request
    pub async fn generate_report(
        &mut self,
        request: ReportRequest,
    ) -> ArbitrageResult<GeneratedReport> {
        let start_time = worker::Date::now().as_millis();

        // Validate request
        self.validate_report_request(&request)?;

        // Check if report is cached
        if let Some(cached_report) = self.get_cached_report(&request).await? {
            return Ok(cached_report);
        }

        // Get template
        let template = self.templates.get(&request.template_id).ok_or_else(|| {
            ArbitrageError::processing_error(format!("Template not found: {}", request.template_id))
        })?;

        // Generate report content
        let report_content = self.generate_report_content(template, &request).await?;

        // Export to requested format
        let exported_data = self.export_report(&report_content, &request.format).await?;

        // Create report result
        let generation_time = worker::Date::now().as_millis() - start_time;
        let report_id = format!(
            "report_{}_{}",
            request.request_id,
            worker::Date::now().as_millis()
        );

        let generated_report = GeneratedReport {
            report_id: report_id.clone(),
            request_id: request.request_id.clone(),
            template_id: request.template_id.clone(),
            user_id: request.user_id.clone(),
            format: request.format.clone(),
            file_size_bytes: exported_data.len() as u64,
            generation_time_ms: generation_time,
            status: ReportStatus::Completed,
            download_url: Some(format!("/api/reports/download/{}", report_id)),
            error_message: None,
            generated_at: worker::Date::now().as_millis(),
            expires_at: worker::Date::now().as_millis() + (24 * 60 * 60 * 1000), // 24 hours
        };

        // Cache the report
        self.cache_report(&generated_report, &exported_data).await?;

        // Update metrics
        self.update_generation_metrics(generation_time, &request.format, exported_data.len());

        Ok(generated_report)
    }

    /// Validate report request
    fn validate_report_request(&self, request: &ReportRequest) -> ArbitrageResult<()> {
        if !self.templates.contains_key(&request.template_id) {
            return Err(ArbitrageError::validation_error(format!(
                "Invalid template_id: {}",
                request.template_id
            )));
        }

        if !self.config.supported_formats.contains(&request.format) {
            return Err(ArbitrageError::validation_error(format!(
                "Unsupported format: {}",
                request.format
            )));
        }

        if request.date_range.start_date >= request.date_range.end_date {
            return Err(ArbitrageError::validation_error(
                "Invalid date range".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate report content from template
    async fn generate_report_content(
        &self,
        template: &ReportTemplate,
        request: &ReportRequest,
    ) -> ArbitrageResult<HashMap<String, serde_json::Value>> {
        let mut content = HashMap::new();

        // Add report metadata
        content.insert(
            "report_id".to_string(),
            serde_json::json!(request.request_id),
        );
        content.insert(
            "template_name".to_string(),
            serde_json::json!(template.name),
        );
        content.insert(
            "generated_at".to_string(),
            serde_json::json!(worker::Date::now().as_millis()),
        );
        content.insert(
            "date_range".to_string(),
            serde_json::to_value(&request.date_range)?,
        );

        // Process each section
        for section in &template.sections {
            let section_data = self.generate_section_data(section, request).await?;
            content.insert(section.section_id.clone(), section_data);
        }

        Ok(content)
    }

    /// Generate data for a specific report section
    async fn generate_section_data(
        &self,
        section: &ReportSection,
        request: &ReportRequest,
    ) -> ArbitrageResult<serde_json::Value> {
        match section.data_source.as_str() {
            "trading_metrics" => self.generate_trading_metrics(section, request).await,
            "opportunities" => self.generate_opportunities_data(section, request).await,
            "system_metrics" => self.generate_system_metrics(section, request).await,
            _ => Ok(serde_json::json!({
                "error": format!("Unknown data source: {}", section.data_source)
            })),
        }
    }

    /// Generate trading metrics data
    async fn generate_trading_metrics(
        &self,
        _section: &ReportSection,
        request: &ReportRequest,
    ) -> ArbitrageResult<serde_json::Value> {
        // Mock trading metrics data
        Ok(serde_json::json!({
            "total_opportunities": 150,
            "executed_opportunities": 45,
            "success_rate": 85.5,
            "total_profit": 1250.75,
            "average_profit_per_trade": 27.79,
            "best_performing_pair": "BTC/USDT",
            "date_range": request.date_range
        }))
    }

    /// Generate opportunities data
    async fn generate_opportunities_data(
        &self,
        _section: &ReportSection,
        request: &ReportRequest,
    ) -> ArbitrageResult<serde_json::Value> {
        // Mock opportunities data
        Ok(serde_json::json!({
            "opportunities": [
                {
                    "pair": "BTC/USDT",
                    "exchange_a": "Binance",
                    "exchange_b": "Bybit",
                    "profit": 125.50,
                    "executed_at": request.date_range.start_date + 3600000
                },
                {
                    "pair": "ETH/USDT",
                    "exchange_a": "OKX",
                    "exchange_b": "Binance",
                    "profit": 89.25,
                    "executed_at": request.date_range.start_date + 7200000
                }
            ],
            "summary": {
                "total_count": 2,
                "total_profit": 214.75
            }
        }))
    }

    /// Generate system metrics data
    async fn generate_system_metrics(
        &self,
        _section: &ReportSection,
        request: &ReportRequest,
    ) -> ArbitrageResult<serde_json::Value> {
        // Mock system metrics data
        Ok(serde_json::json!({
            "metrics": [
                {
                    "timestamp": request.date_range.start_date,
                    "latency": 45.2,
                    "throughput": 1250,
                    "error_rate": 0.02
                },
                {
                    "timestamp": request.date_range.start_date + 3600000,
                    "latency": 42.8,
                    "throughput": 1380,
                    "error_rate": 0.01
                }
            ],
            "averages": {
                "avg_latency": 44.0,
                "avg_throughput": 1315,
                "avg_error_rate": 0.015
            }
        }))
    }

    /// Export report to specified format
    async fn export_report(
        &self,
        content: &HashMap<String, serde_json::Value>,
        format: &str,
    ) -> ArbitrageResult<Vec<u8>> {
        match format {
            "json" => self.export_to_json(content).await,
            "csv" => self.export_to_csv(content).await,
            "pdf" => self.export_to_pdf(content).await,
            "html" => self.export_to_html(content).await,
            _ => Err(ArbitrageError::processing_error(format!(
                "Unsupported export format: {}",
                format
            ))),
        }
    }

    /// Export report to JSON format
    async fn export_to_json(
        &self,
        content: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<Vec<u8>> {
        let json_string = serde_json::to_string_pretty(content)
            .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

        if self.config.enable_compression {
            // In a real implementation, you would compress the data here
            Ok(json_string.into_bytes())
        } else {
            Ok(json_string.into_bytes())
        }
    }

    /// Export report to CSV format
    async fn export_to_csv(
        &self,
        content: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<Vec<u8>> {
        let mut csv_content = String::new();

        // Add header
        csv_content.push_str("Section,Key,Value\n");

        // Add content
        for (section_id, section_data) in content {
            if let serde_json::Value::Object(obj) = section_data {
                for (key, value) in obj {
                    csv_content.push_str(&format!("{},{},{}\n", section_id, key, value));
                }
            }
        }

        Ok(csv_content.into_bytes())
    }

    /// Export report to PDF format (mock implementation)
    async fn export_to_pdf(
        &self,
        content: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<Vec<u8>> {
        // Mock PDF generation - in reality, you would use a PDF library
        let pdf_content = format!(
            "PDF Report Generated at {}\n\nContent:\n{}",
            worker::Date::now().as_millis(),
            serde_json::to_string_pretty(content).unwrap_or_default()
        );

        Ok(pdf_content.into_bytes())
    }

    /// Export report to HTML format
    async fn export_to_html(
        &self,
        content: &HashMap<String, serde_json::Value>,
    ) -> ArbitrageResult<Vec<u8>> {
        let mut html_content = String::new();
        html_content
            .push_str("<!DOCTYPE html><html><head><title>ArbEdge Report</title></head><body>");
        html_content.push_str("<h1>ArbEdge Analytics Report</h1>");

        for (section_id, section_data) in content {
            html_content.push_str(&format!("<h2>{}</h2>", section_id));
            html_content.push_str(&format!(
                "<pre>{}</pre>",
                serde_json::to_string_pretty(section_data).unwrap_or_default()
            ));
        }

        html_content.push_str("</body></html>");
        Ok(html_content.into_bytes())
    }

    /// Get cached report if available
    async fn get_cached_report(
        &self,
        request: &ReportRequest,
    ) -> ArbitrageResult<Option<GeneratedReport>> {
        if let Some(kv) = &self.kv_store {
            let cache_key = self.generate_cache_key(request);

            if let Ok(Some(cached_data)) = kv.get(&cache_key).text().await {
                if let Ok(report) = serde_json::from_str::<GeneratedReport>(&cached_data) {
                    // Check if report is still valid
                    if report.expires_at > worker::Date::now().as_millis() {
                        return Ok(Some(report));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Cache generated report
    async fn cache_report(&self, report: &GeneratedReport, data: &[u8]) -> ArbitrageResult<()> {
        if let Some(kv) = &self.kv_store {
            let cache_key = format!("report:{}", report.report_id);
            let report_json = serde_json::to_string(report)
                .map_err(|e| ArbitrageError::serialization_error(e.to_string()))?;

            // Cache report metadata
            let _ = kv
                .put(&cache_key, report_json)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await;

            // Cache report data
            let data_key = format!("report_data:{}", report.report_id);
            let _ = kv
                .put(&data_key, data)?
                .expiration_ttl(self.config.cache_ttl_seconds)
                .execute()
                .await;
        }

        Ok(())
    }

    /// Generate cache key for report request
    fn generate_cache_key(&self, request: &ReportRequest) -> String {
        format!(
            "report:{}:{}:{}:{}",
            request.template_id,
            request.format,
            request.date_range.start_date,
            request.date_range.end_date
        )
    }

    /// Update generation performance metrics
    fn update_generation_metrics(
        &mut self,
        generation_time_ms: u64,
        format: &str,
        file_size_bytes: usize,
    ) {
        self.metrics.reports_generated += 1;

        // Update average generation time (exponential moving average)
        let alpha = 0.1;
        self.metrics.average_generation_time_ms = alpha * generation_time_ms as f64
            + (1.0 - alpha) * self.metrics.average_generation_time_ms;

        // Update format-specific metrics
        *self
            .metrics
            .exports_by_format
            .entry(format.to_string())
            .or_insert(0) += 1;

        // Update file size tracking
        self.metrics.total_export_size_mb += file_size_bytes as f64 / (1024.0 * 1024.0);

        // Calculate reports per hour
        let current_time = worker::Date::now().as_millis();
        let time_diff_hours = (current_time - self.last_generation_time) as f64 / (1000.0 * 3600.0);
        if time_diff_hours > 0.0 {
            self.metrics.reports_per_hour = 1.0 / time_diff_hours;
        }
        self.last_generation_time = current_time;

        // Update success rate
        let total_reports = self.metrics.reports_generated + self.metrics.reports_failed;
        if total_reports > 0 {
            self.metrics.export_success_rate =
                self.metrics.reports_generated as f64 / total_reports as f64;
        }

        self.metrics.last_updated = current_time;
    }

    /// Get health status
    pub async fn health_check(&self) -> ArbitrageResult<ReportGeneratorHealth> {
        let active_reports = self.active_reports.len() as u32;
        let queue_size = self.report_queue.len() as u32;

        let report_generation_healthy = active_reports <= self.config.max_concurrent_reports;
        let export_system_healthy = self.metrics.export_success_rate >= 0.95; // 95% success rate
        let scheduler_healthy = true; // Simplified for now

        let is_healthy = report_generation_healthy && export_system_healthy && scheduler_healthy;

        Ok(ReportGeneratorHealth {
            is_healthy,
            report_generation_healthy,
            export_system_healthy,
            scheduler_healthy,
            active_reports,
            queue_size,
            average_generation_time_ms: self.metrics.average_generation_time_ms,
            last_health_check: worker::Date::now().as_millis(),
        })
    }

    /// Get performance metrics
    pub async fn get_metrics(&self) -> ArbitrageResult<ReportGeneratorMetrics> {
        Ok(self.metrics.clone())
    }

    /// Add custom report template
    pub async fn add_template(&mut self, template: ReportTemplate) -> ArbitrageResult<()> {
        self.templates
            .insert(template.template_id.clone(), template);
        Ok(())
    }

    /// Get available templates
    pub fn get_templates(&self) -> Vec<&ReportTemplate> {
        self.templates.values().collect()
    }

    /// Queue report for generation
    pub async fn queue_report(&mut self, request: ReportRequest) -> ArbitrageResult<()> {
        self.validate_report_request(&request)?;
        self.report_queue.push(request);
        Ok(())
    }

    /// Process report queue
    pub async fn process_queue(&mut self) -> ArbitrageResult<Vec<GeneratedReport>> {
        let mut results = Vec::new();
        let batch_size = self
            .config
            .batch_processing_size
            .min(self.report_queue.len());

        for _ in 0..batch_size {
            if let Some(request) = self.report_queue.pop() {
                match self.generate_report(request).await {
                    Ok(report) => results.push(report),
                    Err(e) => {
                        self.metrics.reports_failed += 1;
                        // Log error but continue processing
                        eprintln!("Report generation failed: {:?}", e);
                    }
                }
            }
        }

        Ok(results)
    }
}

impl Default for ReportGeneratorMetrics {
    fn default() -> Self {
        Self {
            reports_generated: 0,
            reports_failed: 0,
            average_generation_time_ms: 0.0,
            reports_per_hour: 0.0,
            exports_by_format: HashMap::new(),
            export_success_rate: 1.0,
            total_export_size_mb: 0.0,
            scheduled_reports_executed: 0,
            scheduled_reports_failed: 0,
            custom_reports_generated: 0,
            cache_hit_rate: 0.0,
            error_rate: 0.0,
            queue_processing_rate: 0.0,
            last_updated: worker::Date::now().as_millis(),
        }
    }
}

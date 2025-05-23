// src/services/positions.rs

use crate::types::{ArbitragePosition, ExchangeIdEnum, PositionSide, PositionStatus};
use crate::utils::{ArbitrageError, ArbitrageResult};
use worker::kv::KvStore;

pub struct PositionsService {
    kv_store: KvStore,
}

impl PositionsService {
    pub fn new(kv_store: KvStore) -> Self {
        Self { kv_store }
    }

    pub async fn create_position(
        &self,
        position_data: CreatePositionData,
    ) -> ArbitrageResult<ArbitragePosition> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis() as u64;

        let position = ArbitragePosition {
            id: id.clone(),
            exchange: position_data.exchange,
            pair: position_data.pair,
            side: position_data.side,
            size: position_data.size,
            entry_price: position_data.entry_price,
            current_price: None,
            pnl: None,
            status: PositionStatus::Open,
            created_at: now,
            updated_at: now,
        };

        // Store position
        let key = format!("position:{}", id);
        let value = serde_json::to_string(&position).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize position: {}", e))
        })?;

        self.kv_store
            .put(&key, value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to store position: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute KV put: {}", e))
            })?;

        // Update position index
        self.add_to_position_index(&id).await?;

        Ok(position)
    }

    pub async fn get_position(&self, id: &str) -> ArbitrageResult<Option<ArbitragePosition>> {
        let key = format!("position:{}", id);

        match self.kv_store.get(&key).text().await {
            Ok(Some(value)) => {
                let position: ArbitragePosition = serde_json::from_str(&value).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to deserialize position: {}", e))
                })?;
                Ok(Some(position))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ArbitrageError::database_error(format!(
                "Failed to get position: {}",
                e
            ))),
        }
    }

    pub async fn update_position(
        &self,
        id: &str,
        update_data: UpdatePositionData,
    ) -> ArbitrageResult<Option<ArbitragePosition>> {
        let mut position = match self.get_position(id).await? {
            Some(pos) => pos,
            None => return Ok(None),
        };

        // Update fields if provided
        if let Some(size) = update_data.size {
            position.size = size;
        }
        if let Some(current_price) = update_data.current_price {
            position.current_price = Some(current_price);
        }
        if let Some(pnl) = update_data.pnl {
            position.pnl = Some(pnl);
        }
        if let Some(status) = update_data.status {
            position.status = status;
        }

        position.updated_at = chrono::Utc::now().timestamp_millis() as u64;

        // Store updated position
        let key = format!("position:{}", id);
        let value = serde_json::to_string(&position).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize position: {}", e))
        })?;

        self.kv_store
            .put(&key, value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to store position: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute KV put: {}", e))
            })?;

        Ok(Some(position))
    }

    pub async fn close_position(&self, id: &str) -> ArbitrageResult<bool> {
        let update_data = UpdatePositionData {
            size: None,
            current_price: None,
            pnl: None,
            status: Some(PositionStatus::Closed),
        };

        match self.update_position(id, update_data).await? {
            Some(_) => Ok(true),
            None => Ok(false),
        }
    }

    pub async fn get_all_positions(&self) -> ArbitrageResult<Vec<ArbitragePosition>> {
        // Get position IDs from index
        let position_ids = self.get_position_index().await?;
        let mut positions = Vec::new();

        for id in position_ids {
            if let Some(position) = self.get_position(&id).await? {
                positions.push(position);
            }
        }

        Ok(positions)
    }

    pub async fn get_open_positions(&self) -> ArbitrageResult<Vec<ArbitragePosition>> {
        let all_positions = self.get_all_positions().await?;
        Ok(all_positions
            .into_iter()
            .filter(|pos| pos.status == PositionStatus::Open)
            .collect())
    }

    pub async fn calculate_total_pnl(&self) -> ArbitrageResult<f64> {
        let positions = self.get_open_positions().await?;
        let total_pnl = positions.iter().filter_map(|pos| pos.pnl).sum();
        Ok(total_pnl)
    }

    // Helper methods for position index management
    async fn get_position_index(&self) -> ArbitrageResult<Vec<String>> {
        match self.kv_store.get("positions:index").text().await {
            Ok(Some(value)) => {
                let ids: Vec<String> = serde_json::from_str(&value).map_err(|e| {
                    ArbitrageError::parse_error(format!("Failed to deserialize position index: {}", e))
                })?;
                Ok(ids)
            }
            Ok(None) => Ok(Vec::new()),
            Err(e) => Err(ArbitrageError::database_error(format!(
                "Failed to get position index: {}",
                e
            ))),
        }
    }

    async fn add_to_position_index(&self, position_id: &str) -> ArbitrageResult<()> {
        let mut index = self.get_position_index().await?;
        if !index.contains(&position_id.to_string()) {
            index.push(position_id.to_string());
            self.save_position_index(&index).await?;
        }
        Ok(())
    }

    async fn remove_from_position_index(&self, position_id: &str) -> ArbitrageResult<()> {
        let mut index = self.get_position_index().await?;
        index.retain(|id| id != position_id);
        self.save_position_index(&index).await?;
        Ok(())
    }

    async fn save_position_index(&self, index: &[String]) -> ArbitrageResult<()> {
        let value = serde_json::to_string(index).map_err(|e| {
            ArbitrageError::parse_error(format!("Failed to serialize position index: {}", e))
        })?;

        self.kv_store
            .put("positions:index", value)
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to store position index: {}", e))
            })?
            .execute()
            .await
            .map_err(|e| {
                ArbitrageError::database_error(format!("Failed to execute position index put: {}", e))
            })?;

        Ok(())
    }
}

// Helper structs for position operations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreatePositionData {
    pub exchange: ExchangeIdEnum,
    pub pair: String,
    pub side: PositionSide,
    pub size: f64,
    pub entry_price: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdatePositionData {
    pub size: Option<f64>,
    pub current_price: Option<f64>,
    pub pnl: Option<f64>,
    pub status: Option<PositionStatus>,
}

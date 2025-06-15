-- Migration 015: Add profit_percentage column to opportunities table
-- Created: 2025-01-27
-- Description: Add missing profit_percentage column that is used throughout the codebase

-- Add profit_percentage column to opportunities table
ALTER TABLE opportunities ADD COLUMN profit_percentage REAL DEFAULT 0.0;

-- Create index for performance on profit_percentage queries
CREATE INDEX IF NOT EXISTS idx_opportunities_profit_percentage ON opportunities(profit_percentage);

-- Update existing opportunities to calculate profit_percentage from rate_difference
-- This ensures backward compatibility with existing data
UPDATE opportunities 
SET profit_percentage = COALESCE(rate_difference, 0.0) 
WHERE profit_percentage IS NULL OR profit_percentage = 0.0;

-- Record this migration
INSERT INTO schema_migrations (version, description) 
VALUES ('015', 'Add profit_percentage column to opportunities table'); 
-- Migration: Create opportunity history table for analytics and ML training
-- Purpose: Store historical arbitrage data for pattern recognition and strategy optimization

CREATE TABLE IF NOT EXISTS opportunity_history (
    id TEXT PRIMARY KEY,
    symbol TEXT NOT NULL,
    exchange_from TEXT NOT NULL,
    exchange_to TEXT NOT NULL,
    profit_percentage REAL NOT NULL,
    rate_difference REAL NOT NULL,
    volume_24h REAL,
    price_from REAL NOT NULL,
    price_to REAL NOT NULL,
    confidence_score INTEGER DEFAULT 50,
    market_conditions TEXT, -- JSON string with market data
    execution_attempted BOOLEAN DEFAULT FALSE,
    execution_success BOOLEAN DEFAULT FALSE,
    execution_profit_actual REAL,
    created_at INTEGER NOT NULL, -- Unix timestamp
    archived_at INTEGER NOT NULL, -- When moved to history
    expires_at INTEGER,
    funding_rate_from REAL, -- For perpetual futures
    funding_rate_to REAL,
    next_funding_time INTEGER, -- Next funding rate refresh
    opportunity_type TEXT DEFAULT 'spot' -- 'spot', 'perpetual', 'funding_rate'
);

-- Indexes for analytics queries
CREATE INDEX IF NOT EXISTS idx_opportunity_history_symbol ON opportunity_history(symbol);
CREATE INDEX IF NOT EXISTS idx_opportunity_history_created_at ON opportunity_history(created_at);
CREATE INDEX IF NOT EXISTS idx_opportunity_history_profit ON opportunity_history(profit_percentage);
CREATE INDEX IF NOT EXISTS idx_opportunity_history_type ON opportunity_history(opportunity_type);
CREATE INDEX IF NOT EXISTS idx_opportunity_history_execution ON opportunity_history(execution_success);

-- Create funding rates table for perpetual futures arbitrage
CREATE TABLE IF NOT EXISTS funding_rates (
    id TEXT PRIMARY KEY,
    exchange TEXT NOT NULL,
    symbol TEXT NOT NULL,
    funding_rate REAL NOT NULL,
    next_funding_time INTEGER NOT NULL,
    predicted_rate REAL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_funding_rates_exchange_symbol ON funding_rates(exchange, symbol);
CREATE INDEX IF NOT EXISTS idx_funding_rates_next_funding ON funding_rates(next_funding_time);

-- Create analytics summary table for performance tracking
CREATE TABLE IF NOT EXISTS opportunity_analytics (
    id TEXT PRIMARY KEY,
    date_key TEXT NOT NULL, -- YYYY-MM-DD format
    symbol TEXT NOT NULL,
    total_opportunities INTEGER DEFAULT 0,
    avg_profit_percentage REAL DEFAULT 0.0,
    max_profit_percentage REAL DEFAULT 0.0,
    execution_success_rate REAL DEFAULT 0.0,
    total_volume REAL DEFAULT 0.0,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_opportunity_analytics_date ON opportunity_analytics(date_key);
CREATE INDEX IF NOT EXISTS idx_opportunity_analytics_symbol ON opportunity_analytics(symbol); 
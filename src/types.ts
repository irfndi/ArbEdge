import type { Logger as WinstonLogger } from "winston";
import type {
  Exchange as CCXTExchange,
  Market as CCXTMarket,
  OrderBook as CCXTOrderBook,
  Trade as CCXTTrade,
  Ticker as CCXTTicker,
  OHLCV as CCXTOHLCV,
  Balances as CCXTBalances,
  Position as CCXTPosition,
  Order as CCXTOrder,
  Transaction as CCXTTransaction,
  DepositAddress as CCXTDepositAddress,
  WithdrawalResponse as CCXTWithdrawalResponse,
  FundingRate as CCXTFundingRate,
  FundingRateHistory as CCXTFundingRateHistory,
  LeverageTier as CCXTLeverageTier,
  MarketInterface as CCXTMarketInterface,
  TradingFeeInterface as CCXTTradingFeeInterface,
  TradingFees as CCXTTradingFees,
} from "ccxt";

// General Utility Types
export interface LoggerInterface {
  debug(message: string, ...meta: unknown[]): void;
  info(message: string, ...meta: unknown[]): void;
  warn(message: string, ...meta: unknown[]): void;
  error(message: string, ...meta: unknown[]): void;
  log(level: string, message: string, ...meta: unknown[]): void;
  addError?(error: Error, ...meta: unknown[]): void;
  addContext?(key: string, value: unknown): void;
  http?(message: string, ...meta: unknown[]): void;
  verbose?(message: string, ...meta: unknown[]): void;
  silly?(message: string, ...meta: unknown[]): void;
  child?(options: Record<string, unknown>): LoggerInterface;
}

// Exchange & Trading Related Types
export type KnownExchangeId =
  | "binance"
  | "bybit"
  | "kraken"
  | "okx"
  | "bingx"
  | "bitget"
  | "mexc";
// For custom exchange IDs, use a branded string type to maintain type safety.
// Example: const myCustomExchange = 'myNewExchange' as CustomExchangeId;
export type CustomExchangeId = string & {
  readonly __brand: "CustomExchangeId";
};
export type ExchangeId = KnownExchangeId | CustomExchangeId;

export type TradingPairSymbol = string; // e.g., "BTC/USDT"

export interface StructuredTradingPair {
  symbol: TradingPairSymbol; // e.g., "BTC/USDT"
  base: string; // e.g., "BTC"
  quote: string; // e.g., "USDT"
  type: "spot" | "swap" | "future"; // Added type property
}

export type FundingRateInfo = Partial<
  Omit<
    CCXTFundingRate,
    | "symbol"
    | "timestamp"
    | "datetime"
    | "fundingRate"
    | "markPrice"
    | "indexPrice"
    | "estimatedSettlePrice"
    | "fundingTimestamp"
    | "fundingTime"
    | "nextFundingTimestamp"
    | "nextFundingTime"
  >
> & {
  symbol?: StructuredTradingPair; // Optional to support both formats
  pair?: string; // Added for test compatibility
  exchange: ExchangeId;
  fundingRate: number; // Ensure this is always present
  timestamp: number; // Timestamp when this data was fetched/calculated. CCXT's timestamp is usually for the data itself.
  datetime?: string; // ISO8601 string of timestamp (for compatibility with tests)
  // Optional fields from CCXTFundingRate that we might want to ensure or re-type if necessary
  markPrice?: number;
  indexPrice?: number;
  estimatedSettlePrice?: number;
  fundingTimestamp?: number; // Timestamp of the funding event (if provided by exchange)
  fundingTime?: string; // ISO8601 string of fundingTimestamp
  nextFundingTimestamp?: number; // Timestamp of the next funding event
  nextFundingTime?: string; // ISO8601 string of nextFundingTimestamp
  info?: Record<string, unknown>; // Added info field that appears in code
};

export interface OrderBookLevel {
  price: number;
  amount: number;
}

export interface OrderBookInfo {
  exchange: ExchangeId;
  pair: TradingPairSymbol;
  bids: OrderBookLevel[]; // Best bids (highest prices)
  asks: OrderBookLevel[]; // Best asks (lowest prices)
  timestamp: number; // Timestamp of the snapshot
}

export interface ArbitrageOpportunity {
  // Fields from OpportunityServiceSpec.md (ensure these are primary for funding rate opportunities)
  pair: TradingPairSymbol;
  longExchange: ExchangeId;
  shortExchange: ExchangeId;
  longRate: number;
  shortRate: number;
  rateDifference: number;
  longExchangeTakerFeeRate: number;
  shortExchangeTakerFeeRate: number;
  totalEstimatedFees: number;
  netRateDifference: number;
  timestamp: number;

  // Existing/General fields from types.ts (making them optional or to be reviewed/namespaced)
  id?: string;
  type?: string;
  // pairSymbol?: TradingPairSymbol; // Effectively replaced by 'pair', could be removed if no other service relies on it by this name.
  // For now, commenting out to prefer 'pair'. If build breaks elsewhere, it needs addressing there.

  buyPrice?: number;
  sellPrice?: number;

  // These metrics are now covered by spec-specific names above for funding rate opps
  // grossProfitMetric?: number;
  // netProfitMetric?: number;
  profitPercentage?: number;
  potentialProfitValue?: number;

  // These fee rates are now covered by spec-specific names above
  // feeRateBuy?: number;
  // feeRateSell?: number;
  // estimatedTotalFees?: number; // This was for monetary value, spec has 'totalEstimatedFees' as sum of rates

  profitableAsset?: string;
  details?: string;
  minTradeableAmount?: number;
  maxTradeableAmount?: number;

  tradeExecutionDetails?: {
    exchangeBuy: ExchangeId;
    exchangeSell: ExchangeId;
    pair: TradingPairSymbol;
    buyPrice: number;
    sellPrice: number;
    amount: number;
    estimatedProfit: number;
  };
  info?: Record<string, unknown>;
}

// CCXT Passthrough Types (re-exporting for convenience or augmentation if needed)
export type Market = CCXTMarket;
export type Order = CCXTOrder;
export type Position = CCXTPosition;
// CCXTBalances (imported as CCXTBalances and re-exported as Balances) is typically:
// export interface Balances { [key: string]: BalanceEntryStruct; } // where BalanceEntryStruct has free, used, total
// So, a single balance entry can be derived from CCXTBalances.
export type BalanceEntry = CCXTBalances[string];
// 'Balance' will now refer to a single currency's balance structure.
export type Balance = BalanceEntry;
export type Balances = CCXTBalances;
export type Ticker = CCXTTicker;
export type OrderBook = CCXTOrderBook;
export type Trade = CCXTTrade;
export type Transaction = CCXTTransaction;
export type DepositAddress = CCXTDepositAddress;
export type WithdrawalResponse = CCXTWithdrawalResponse;
export type TradingFeeInterface = CCXTTradingFeeInterface;
export type OHLCV = CCXTOHLCV;

// Environment Variable Structure for Cloudflare Worker
export interface Env {
  // KV Namespaces
  ArbEdgeKV: KVNamespace; // For general app config, API keys if not directly in env

  // Durable Object Namespaces (example, adjust as per actual usage)
  POSITIONS: DurableObjectNamespace; // If using DO for position tracking
  // Add other DOs if any: USER_SESSIONS: DurableObjectNamespace;

  // Secrets (typically set in Cloudflare dashboard or via wrangler secrets)
  TELEGRAM_BOT_TOKEN: string;
  TELEGRAM_CHAT_ID: string;

  // Configuration strings
  EXCHANGES: string; // Comma-separated list of exchange IDs, e.g., "binance,kraken"
  ARBITRAGE_THRESHOLD: string; // Minimum arbitrage percentage, e.g., "0.5" for 0.5%
  MONITORED_PAIRS_CONFIG: string; // JSON string for structured trading pairs

  LOG_LEVEL?: string; // Optional: 'debug', 'info', 'warn', 'error'
  LOGGER?: LoggerInterface; // For passing a logger instance

  // Optional: Direct API keys (less secure, prefer KV or secrets manager)
  BINANCE_API_KEY?: string;
  BINANCE_API_SECRET?: string;
  KRAKEN_API_KEY?: string;
  KRAKEN_API_SECRET?: string;
  BYBIT_API_KEY?: string;
  BYBIT_API_SECRET?: string;
  OKX_API_KEY?: string;
  OKX_API_SECRET?: string;
  BINGX_API_KEY?: string;
  BINGX_API_SECRET?: string;
  BITGET_API_KEY?: string;
  BITGET_API_SECRET?: string;
  MEXC_API_KEY?: string;
  MEXC_API_SECRET?: string;
  // Add other exchange keys or specific config as needed
}

// Specific for TelegramService signals if needed beyond ArbitrageOpportunity
export interface TradeSignal {
  pair: TradingPairSymbol;
  action: "LONG" | "SHORT" | "CLOSE_LONG" | "CLOSE_SHORT";
  exchange: ExchangeId;
  price?: number;
  stopLoss?: number;
  takeProfit?: number;
  notes?: string;
  timestamp: number;
}

export type {
  CCXTExchange,
  CCXTMarket,
  CCXTOrderBook,
  CCXTTrade,
  CCXTTicker,
  CCXTOHLCV,
  CCXTBalances,
  CCXTPosition,
  CCXTOrder,
  CCXTTransaction,
  CCXTDepositAddress,
  CCXTWithdrawalResponse,
  CCXTFundingRateHistory,
  CCXTLeverageTier,
  CCXTMarketInterface,
  CCXTTradingFeeInterface,
  CCXTTradingFees,
};

export type LogLevel =
  | "error"
  | "warn"
  | "info"
  | "http"
  | "verbose"
  | "debug"
  | "silly";

// Configuration specific to TelegramService
export interface TelegramConfig {
  botToken: string;
  chatId: string;
  logger: LoggerInterface;
}

// Define other specific configuration interfaces as needed, e.g., for exchanges
// export interface ExchangeClientConfig { ... }

// End of file

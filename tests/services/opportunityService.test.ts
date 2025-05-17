/// <reference types="vitest/globals" />
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import type { Mock, Mocked } from "vitest";
import type {
  TradingPairSymbol,
  ArbitrageOpportunity,
  FundingRateInfo,
  StructuredTradingPair,
  ExchangeId,
  Env,
  LoggerInterface,
} from "../../src/types.ts";
import type * as ccxt from "ccxt";
import type { KVNamespacePutOptions } from "@cloudflare/workers-types";
import { TelegramService } from "../../src/services/telegramService";
import type { TelegramService as OriginalTelegramServiceClass } from "../../src/services/telegramService";
import { ExchangeService } from "../../src/services/exchangeService";
import type {
  ExchangeService as OriginalExchangeServiceClass,
  ExchangeServiceConfig,
  IExchangeFromCcxt as OriginalIExchange,
} from "../../src/services/exchangeService";
import {
  OpportunityService,
  type OpportunityServiceConfig,
} from "../../src/services/opportunityService";
import type { TelegramConfig } from "../../src/types.ts";

// --- BEGIN LOGGER MOCK ---
const fullyMockedLogger: Mocked<LoggerInterface> = {
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  log: vi.fn(),
};

vi.mock("../../src/utils/logger", () => ({
  createLogger: vi.fn(() => fullyMockedLogger),
}));

// --- END LOGGER MOCK ---

const OriginalExchangeService = ExchangeService;
const OriginalTelegramService = TelegramService;

const mockGetFundingRate: Mock<
  (
    ...args: [exchangeId: string, symbol: string]
  ) => Promise<FundingRateInfo | null>
> = vi.fn();
mockGetFundingRate.mockResolvedValue(null);

const mockGetFundingRates = vi.fn();

const mockGetTradingFees: Mock<
  (
    ...args: [exchangeId: string, symbol: string]
  ) => Promise<ccxt.TradingFeeInterface | null>
> = vi.fn();
mockGetTradingFees.mockResolvedValue(null);

const mockGetExchange = vi.fn();
const mockGetMarkets = vi.fn();
const mockGetMarket = vi.fn();
const mockGetTradingFee = vi.fn();
const mockSetLeverage = vi.fn();
const mockCreateOrder = vi.fn();
const mockGetOpenPositions = vi.fn();
const mockClosePosition = vi.fn();
const mockGetTicker = vi.fn();
const mockGetTickers = vi.fn();
const mockLoadMarkets = vi.fn().mockResolvedValue({});
const mockInitializeDefaultLeverage = vi.fn().mockResolvedValue(undefined);
const mockGetCcxtInstance = vi.fn();

const singleMockExchangeServiceInstance = {
  logger: fullyMockedLogger,
  config: {} as ExchangeServiceConfig,
  getFundingRate: mockGetFundingRate,
  getFundingRates: mockGetFundingRates,
  getTradingFees: mockGetTradingFees,
  getExchange: mockGetExchange,
  getMarkets: mockGetMarkets,
  getMarket: mockGetMarket,
  getTradingFee: mockGetTradingFee,
  setLeverage: mockSetLeverage,
  createOrder: mockCreateOrder,
  getOpenPositions: mockGetOpenPositions,
  closePosition: mockClosePosition,
  getTicker: mockGetTicker,
  getTickers: mockGetTickers,
  loadMarkets: mockLoadMarkets,
  initializeDefaultLeverage: mockInitializeDefaultLeverage,
  getCcxtInstance: mockGetCcxtInstance,
};

vi.mock("../../src/services/exchangeService", () => {
  const MockExchangeService = vi
    .fn()
    .mockImplementation((config: ExchangeServiceConfig) => {
      singleMockExchangeServiceInstance.config = config;
      singleMockExchangeServiceInstance.logger = fullyMockedLogger;
      return singleMockExchangeServiceInstance;
    });
  return { ExchangeService: MockExchangeService };
});

vi.mock("../../src/services/telegramService", () => {
  const MockTelegramService = vi.fn().mockImplementation((config) => {
    return {
      logger: fullyMockedLogger,
      config,
      sendOpportunityNotification: vi.fn().mockResolvedValue(undefined),
      sendErrorNotification: vi.fn().mockResolvedValue(undefined),
    };
  });
  return { TelegramService: MockTelegramService };
});

interface MockExchange {
  exchangeId: ExchangeId;
  getFundingRate?: (
    symbol: TradingPairSymbol
  ) => Promise<FundingRateInfo | null>;
  getTradingFees?: (
    symbol: TradingPairSymbol
  ) => Promise<ccxt.TradingFeeInterface | null>;
  getMarket?: (symbol: TradingPairSymbol) => Promise<ccxt.Market | undefined>;
  getTicker?: (symbol: TradingPairSymbol) => Promise<ccxt.Ticker | undefined>;
  has?: {
    fetchFundingRates?: boolean;
    fetchTradingFees?: boolean;
    fetchLeverage?: boolean;
    fetchPositions?: boolean;
    createMarketOrder?: boolean;
    fetchTicker?: boolean;
    fetchBalance?: boolean;
  };
  loadMarkets?: () => Promise<Record<string, ccxt.Market>>;
  fetchMarkets?: () => Promise<ccxt.Market[]>;
  fetchTicker?: (symbol: string) => Promise<ccxt.Ticker | undefined>;
  fetchTickers?: (symbols?: string[]) => Promise<Record<string, ccxt.Ticker>>;
  fetchTradingFees?: () => Promise<Record<string, ccxt.TradingFeeInterface>>;
  createMarketOrder?: (
    symbol: string,
    type: string,
    side: string,
    amount: number,
    price?: number,
    params?: Record<string, unknown>
  ) => Promise<ccxt.Order>;
  setLeverage?: (
    symbol: string,
    leverage: number,
    params?: Record<string, unknown>
  ) => Promise<Record<string, unknown>>;
  fetchPositions?: (
    symbols?: string[],
    params?: Record<string, unknown>
  ) => Promise<ccxt.Position[]>;
  rateLimit?: number;
}

const mockOrder: ccxt.Order = {
  id: "mock-order-id",
  clientOrderId: "mock-client-order-id",
  timestamp: Date.now(),
  datetime: new Date().toISOString(),
  lastTradeTimestamp: 0,
  status: "open",
  symbol: "ETH/USD",
  type: "market",
  side: "buy",
  price: 0,
  amount: 1,
  filled: 0,
  remaining: 1,
  cost: 0,
  fee: { currency: "USD", cost: 0, rate: 0 },
  trades: [],
  info: {},
  average: 0,
  reduceOnly: false,
  postOnly: false,
};

const mockPosition: ccxt.Position = {
  info: {},
  id: "mock-position-id",
  symbol: "ETH/USD:USD",
  timestamp: Date.now(),
  datetime: new Date().toISOString(),
  contracts: 1,
  contractSize: 1,
  side: "long",
  entryPrice: 3000,
  markPrice: 3100,
  notional: 3000,
  leverage: 1,
  unrealizedPnl: 100,
  percentage: undefined,
  collateral: 3000,
  initialMargin: 3000,
  maintenanceMargin: 150,
  marginRatio: undefined,
  liquidationPrice: undefined,
  marginMode: "isolated",
  hedged: false,
  lastUpdateTimestamp: undefined,
  takeProfitPrice: undefined,
  stopLossPrice: undefined,
};

const mockExchangesRecord: Record<ExchangeId, Partial<MockExchange>> = {
  binance: {
    exchangeId: "binance",
    getFundingRate: vi
      .fn()
      .mockImplementation(async (symbol: TradingPairSymbol) => {
        if (symbol === "BTC/USDT")
          return {
            exchange: "binance",
            pair: symbol,
            fundingRate: 0.001,
            timestamp: MOCK_TIMESTAMP,
          };
        if (symbol === "ETH/USDT")
          return {
            exchange: "binance",
            pair: symbol,
            fundingRate: 0.0005,
            timestamp: MOCK_TIMESTAMP,
          };
        return null;
      }),
    getTradingFees: vi.fn(
      async (symbol: string): Promise<ccxt.TradingFeeInterface | null> => {
        if (symbol === "BTC/USDT")
          return {
            symbol,
            taker: 0.001,
            maker: 0.001,
            percentage: true,
            tierBased: false,
            info: {},
          };
        if (symbol === "ETH/USDT")
          return {
            symbol,
            taker: 0.001,
            maker: 0.001,
            percentage: true,
            tierBased: false,
            info: {},
          };
        return null;
      }
    ),
    has: {
      fetchFundingRates: true,
      fetchTradingFees: true,
      fetchTicker: true,
      createMarketOrder: true,
      fetchBalance: true,
    },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.001, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
  bybit: {
    exchangeId: "bybit",
    getFundingRate: vi
      .fn()
      .mockImplementation(async (symbol: TradingPairSymbol) => {
        if (symbol === "BTC/USDT")
          return {
            exchange: "bybit",
            pair: symbol,
            fundingRate: 0.0001,
            timestamp: MOCK_TIMESTAMP,
          };
        if (symbol === "ETH/USDT")
          return {
            exchange: "bybit",
            pair: symbol,
            fundingRate: -0.0002,
            timestamp: MOCK_TIMESTAMP,
          };
        return null;
      }),
    getTradingFees: vi.fn(
      async (symbol: string): Promise<ccxt.TradingFeeInterface | null> => {
        if (symbol === "BTC/USDT")
          return {
            symbol,
            taker: 0.0006,
            maker: 0.0001,
            percentage: true,
            tierBased: false,
            info: {},
          };
        if (symbol === "ETH/USDT")
          return {
            symbol,
            taker: 0.0006,
            maker: 0.0001,
            percentage: true,
            tierBased: false,
            info: {},
          };
        return null;
      }
    ),
    has: {
      fetchFundingRates: true,
      fetchTradingFees: true,
      fetchTicker: true,
      createMarketOrder: true,
      fetchBalance: true,
    },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.001, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
  kraken: {
    exchangeId: "kraken",
    getFundingRate: vi
      .fn()
      .mockImplementation(async (symbol: TradingPairSymbol) => {
        if (symbol === "ADA/USDT")
          return {
            exchange: "kraken",
            pair: symbol,
            fundingRate: 0.001,
            timestamp: MOCK_TIMESTAMP,
          };
        return null;
      }),
    getTradingFees: vi.fn(
      async (symbol: string): Promise<ccxt.TradingFeeInterface | null> => {
        if (symbol === "ADA/USDT")
          return {
            symbol,
            taker: 0.0016,
            maker: 0.001,
            percentage: true,
            tierBased: false,
            info: {},
          };
        return null;
      }
    ),
    has: { fetchFundingRates: true, fetchTradingFees: true },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.002, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
  okx: {
    exchangeId: "okx",
    getFundingRate: vi
      .fn()
      .mockImplementation(async (symbol: TradingPairSymbol) => {
        if (symbol === "ADA/USDT")
          return {
            exchange: "okx",
            pair: symbol,
            fundingRate: 0.0005,
            timestamp: MOCK_TIMESTAMP,
          };
        return null;
      }),
    getTradingFees: vi.fn(
      async (symbol: string): Promise<ccxt.TradingFeeInterface | null> => {
        if (symbol === "ADA/USDT")
          return {
            symbol,
            taker: 0.0008,
            maker: 0.0005,
            percentage: true,
            tierBased: false,
            info: {},
          };
        return null;
      }
    ),
    has: { fetchFundingRates: true, fetchTradingFees: true },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.002, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
  bingx: {
    exchangeId: "bingx",
    getFundingRate: async (symbol: TradingPairSymbol) => null,
    getTradingFees: async (symbol: TradingPairSymbol) => null,
    has: {
      fetchFundingRates: true,
      fetchTradingFees: true,
      fetchLeverage: true,
      fetchPositions: true,
      createMarketOrder: true,
    },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.002, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
  bitget: {
    exchangeId: "bitget",
    getFundingRate: async (symbol: TradingPairSymbol) => null,
    getTradingFees: async (symbol: TradingPairSymbol) => null,
    has: {
      fetchFundingRates: true,
      fetchTradingFees: true,
      fetchLeverage: true,
      fetchPositions: true,
      createMarketOrder: true,
    },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.002, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
  mexc: {
    exchangeId: "mexc",
    getFundingRate: async (symbol: TradingPairSymbol) => null,
    getTradingFees: async (symbol: TradingPairSymbol) => null,
    has: {
      fetchFundingRates: true,
      fetchTradingFees: true,
      fetchLeverage: true,
      fetchPositions: true,
      createMarketOrder: true,
    },
    loadMarkets: vi.fn().mockResolvedValue({
      "ETH/USD": {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    }),
    fetchMarkets: vi.fn().mockResolvedValue([
      {
        id: "ETH/USD",
        symbol: "ETH/USD",
        base: "ETH",
        quote: "USD",
        active: true,
        type: "spot",
      } as ccxt.Market,
    ]),
    fetchTicker: vi.fn().mockResolvedValue(undefined),
    fetchTickers: vi.fn().mockResolvedValue({
      "ETH/USD": { symbol: "ETH/USD", bid: 100, ask: 101 } as ccxt.Ticker,
    }),
    fetchTradingFees: vi
      .fn()
      .mockResolvedValue({ "ETH/USD": { taker: 0.002, maker: 0.001 } }),
    createMarketOrder: vi.fn().mockResolvedValue(mockOrder),
    setLeverage: vi.fn().mockResolvedValue({}),
    fetchPositions: vi.fn().mockResolvedValue([mockPosition]),
  },
};

const createMockKvNamespace = (): KVNamespace => {
  const store = new Map<string, string>();
  return {
    get: vi.fn(
      async (
        key: string,
        type?: "text" | "json" | "arrayBuffer" | "stream"
      ) => {
        if (type && type !== "text" && type !== "json") {
          throw new Error(
            `Mock KV get only supports 'text' or 'json' type, got ${type}`
          );
        }
        const value = store.get(key);
        if (value === undefined) return null;
        if (type === "json") {
          try {
            return JSON.parse(value);
          } catch (e) {
            console.error("Failed to parse JSON from mock KV:", e);
            return null;
          }
        }
        return value;
      }
    ),
    put: vi.fn(
      async (
        key: string,
        value: Record<string, unknown>,
        options?: KVNamespacePutOptions
      ) => {
        store.set(
          key,
          typeof value === "string" ? value : JSON.stringify(value)
        );
      }
    ),
    delete: vi.fn(async (key: string) => {
      store.delete(key);
    }),
    list: vi.fn(async (options?: KVNamespaceListOptions) => {
      const keys: KVNamespaceListKey<undefined>[] = Array.from(
        store.keys()
      ).map(
        (name): KVNamespaceListKey<undefined> => ({
          name,
          metadata: undefined,
          expiration: undefined,
        })
      );
      return { keys, list_complete: true, cursor: undefined };
    }),
    getWithMetadata: vi.fn(
      async (
        key: string,
        type?: "text" | "json" | "arrayBuffer" | "stream"
      ) => {
        if (type && type !== "text" && type !== "json") {
          throw new Error(
            `Mock KV getWithMetadata only supports 'text' or 'json' type, got ${type}`
          );
        }
        const value = store.get(key);
        // biome-ignore lint/suspicious/noExplicitAny: Mock data for KV metadata
        const metadata: any = { lastModified: Date.now() };
        if (value === undefined) return { value: null, metadata: null };
        if (type === "json") {
          try {
            return { value: JSON.parse(value), metadata };
          } catch (e) {
            return { value: null, metadata: null }; // Or throw, depending on desired mock behavior
          }
        }
        return { value, metadata };
      }
    ),
  } as unknown as KVNamespace;
};

const createMockDoNamespace = (): DurableObjectNamespace => {
  return {
    idFromName: vi
      .fn()
      .mockImplementation((name: string) => ({}) as DurableObjectId),
    idFromString: vi
      .fn()
      .mockImplementation((hexId: string) => ({}) as DurableObjectId),
    newUniqueId: vi.fn().mockImplementation(() => ({}) as DurableObjectId),
    get: vi
      .fn()
      .mockImplementation((id: DurableObjectId) => ({}) as DurableObjectStub),
  } as unknown as DurableObjectNamespace;
};

const MOCK_TIMESTAMP = 1672531200000; // 2023-01-01T00:00:00.000Z

const mockEnv: Env = {
  TELEGRAM_BOT_TOKEN: "test-token",
  TELEGRAM_CHAT_ID: "test-chat-id",
  ArbEdgeKV: createMockKvNamespace(),
  POSITIONS: createMockDoNamespace(),
  BINANCE_API_KEY: "test-binance-key",
  BINANCE_API_SECRET: "test-binance-secret",
  BYBIT_API_KEY: "test-bybit-key",
  BYBIT_API_SECRET: "test-bybit-secret",
  KRAKEN_API_KEY: "test-kraken-key",
  KRAKEN_API_SECRET: "test-kraken-secret",
  OKX_API_KEY: "test-okx-key",
  OKX_API_SECRET: "test-okx-secret",
  BINGX_API_KEY: "test-bingx-key",
  BINGX_API_SECRET: "test-bingx-secret",
  BITGET_API_KEY: "test-bitget-key",
  BITGET_API_SECRET: "test-bitget-secret",
  MEXC_API_KEY: "test-mexc-key",
  MEXC_API_SECRET: "test-mexc-secret",
  EXCHANGES: "binance,bybit",
  ARBITRAGE_THRESHOLD: "0.001",
  MONITORED_PAIRS_CONFIG: JSON.stringify([
    { symbol: "BTC/USDT", base: "BTC", quote: "USDT", type: "swap" },
  ]),
  LOG_LEVEL: "debug",
};

describe("OpportunityService", () => {
  let opportunityService: OpportunityService;
  let mockExchangeService: Mocked<OriginalExchangeServiceClass>;
  const mockLogger: Mocked<LoggerInterface> = fullyMockedLogger;

  // Initialize configs after mockLogger is ready
  const baseConfigPart = {
    env: mockEnv,
    logger: mockLogger,
  };

  const credentialsPart = {
    binance: {
      apiKey: "binance-test-key",
      secret: "binance-test-secret",
      defaultLeverage: 10,
      exchangeType: "future",
    },
    bybit: {
      apiKey: "bybit-test-key",
      secret: "bybit-test-secret",
      defaultLeverage: 10,
      exchangeType: "linear",
    },
    kraken: {
      apiKey: "kraken-test-key",
      secret: "kraken-test-secret",
      defaultLeverage: 5,
      exchangeType: "future",
    },
    okx: {
      apiKey: "okx-test-key",
      secret: "okx-test-secret",
      defaultLeverage: 5,
      exchangeType: "future",
    },
    bingx: {
      apiKey: "bingx-test-key",
      secret: "bingx-test-secret",
      defaultLeverage: 5,
      exchangeType: "future",
    },
    bitget: {
      apiKey: "bitget-test-key",
      secret: "bitget-test-secret",
      defaultLeverage: 5,
      exchangeType: "future",
    },
    mexc: {
      apiKey: "mexc-test-key",
      secret: "mexc-test-secret",
      defaultLeverage: 5,
      exchangeType: "future",
    },
  };

  // TODO: Revisit this complex type assignment for ExchangeServiceConfig.
  // The intersection type seems to cause issues with direct assignment or simple casting.
  const mockExchangeServiceConfig = {
    ...baseConfigPart,
    ...credentialsPart,
  } as unknown as ExchangeServiceConfig;

  const mockTelegramServiceConfig: TelegramConfig = {
    botToken: "test-token",
    chatId: "test-chat-id",
    logger: mockLogger as LoggerInterface,
  };

  const DEFAULT_THRESHOLD = 0.0001; // 0.01%

  beforeEach(() => {
    vi.clearAllMocks();

    mockExchangeService = new OriginalExchangeService({
      env: mockEnv,
      logger: mockLogger,
    }) as Mocked<ExchangeService>;

    mockExchangeService.getFundingRate = mockGetFundingRate;
    mockGetFundingRate.mockReset(); // Reset specifically
    mockExchangeService.getTradingFees = mockGetTradingFees;
    mockGetTradingFees.mockReset(); // Reset specifically
    mockExchangeService.getExchangeInstance = mockGetCcxtInstance;
    mockGetCcxtInstance
      .mockReset()
      .mockResolvedValue(createMockCcxtExchange(0.001)); // Default for old tests

    const config: OpportunityServiceConfig = {
      exchangeService: mockExchangeService,
      telegramService: new TelegramService(
        mockTelegramServiceConfig
      ) as Mocked<OriginalTelegramServiceClass>,
      logger: mockLogger,
      monitoredPairs: [
        { symbol: "BTC/USDT", base: "BTC", quote: "USDT", type: "swap" },
      ],
      exchanges: [
        "binance",
        "bybit",
        "kraken",
        "okx",
        "bingx",
        "bitget",
        "mexc",
      ] as ExchangeId[],
      threshold: DEFAULT_THRESHOLD,
    };
    opportunityService = new OpportunityService(config);
  });

  afterEach(() => {
    // vi.clearAllMocks();
  });

  it("should be defined", () => {
    expect(opportunityService).toBeDefined();
  });

  it("should find one opportunity when rate difference exceeds threshold", async () => {
    // Explicit mock for this test to ensure fees are correctly provided
    mockGetTradingFees.mockImplementation(
      async (
        exchangeId: string,
        symbol: string
      ): Promise<ccxt.TradingFeeInterface | null> => {
        if (symbol === "BTC/USDT") {
          if (exchangeId === "binance")
            return {
              symbol: "BTC/USDT",
              taker: 0.0001,
              maker: 0.0001,
              percentage: true,
              tierBased: false,
              info: {},
            };
          if (exchangeId === "bybit")
            return {
              symbol: "BTC/USDT",
              taker: 0.0001,
              maker: 0.0001,
              percentage: true,
              tierBased: false,
              info: {},
            };
        }
        return null;
      }
    );

    const exchangeIds: ExchangeId[] = ["binance", "bybit"];
    const rawPairs: TradingPairSymbol[] = ["BTC/USDT"];
    const threshold = 0.0005;

    mockGetMarkets.mockResolvedValue({
      "BTC/USDT": {
        id: "BTC/USDT",
        symbol: "BTC/USDT",
        base: "BTC",
        quote: "USDT",
        active: true,
        precision: { amount: 8, price: 2 },
        limits: {
          amount: { min: 0.00001, max: 1000 },
          price: { min: 0.01, max: 1000000 },
          cost: { min: 1, max: undefined },
        },
        baseId: "BTC_ID",
        quoteId: "USDT_ID",
        type: "spot",
        spot: true,
        margin: false,
        swap: false,
        future: false,
        option: false,
        contract: false,
        linear: undefined,
        inverse: undefined,
        settle: undefined,
        settleId: undefined,
        contractSize: undefined,
        expiry: undefined,
        expiryDatetime: undefined,
        strike: undefined,
        optionType: undefined,
        info: { test: "marketinfo" },
        percentage: false,
        taker: 0.001,
        maker: 0.001,
        activeFee: false,
      },
    });

    mockGetFundingRate.mockImplementation(
      async (
        exchangeIdInput: string,
        symbol: string
      ): Promise<FundingRateInfo | null> => {
        const exchangeId = exchangeIdInput as ExchangeId;
        if (symbol !== "BTC/USDT") return null;
        if (exchangeId === "binance") {
          return {
            exchange: exchangeId,
            pair: symbol,
            timestamp: MOCK_TIMESTAMP,
            fundingRate: 0.001,
          };
        }
        if (exchangeId === "bybit") {
          return {
            exchange: exchangeId,
            pair: symbol,
            timestamp: MOCK_TIMESTAMP,
            fundingRate: 0.0001,
          };
        }
        return null;
      }
    );

    const opportunities = await opportunityService.findOpportunities(
      exchangeIds,
      rawPairs,
      threshold
    );

    expect(mockGetFundingRate).toHaveBeenCalledTimes(2);
    expect(mockGetFundingRate).toHaveBeenCalledWith("binance", "BTC/USDT");
    expect(mockGetFundingRate).toHaveBeenCalledWith("bybit", "BTC/USDT");

    expect(opportunities).toHaveLength(1);
    const opportunity = opportunities[0];
    expect(opportunity).toMatchObject({
      pair: "BTC/USDT",
      longExchange: "bybit",
      shortExchange: "binance",
      longRate: 0.0001,
      shortRate: 0.001,
      rateDifference: 0.0009,
      longExchangeTakerFeeRate: expect.any(Number),
      shortExchangeTakerFeeRate: expect.any(Number),
      totalEstimatedFees: expect.any(Number),
      netRateDifference: expect.any(Number),
      timestamp: MOCK_TIMESTAMP,
    });
  });

  it("should find no opportunities when rate difference is below threshold", async () => {
    const exchangeIds: ExchangeId[] = ["binance", "bybit"];
    const rawPairs: TradingPairSymbol[] = ["BTC/USDT"];
    const threshold = 0.001;

    mockGetFundingRate.mockImplementation(
      async (
        exchangeIdInput: string,
        symbol: string
      ): Promise<FundingRateInfo | null> => {
        const exchangeId = exchangeIdInput as ExchangeId;
        if (symbol === "BTC/USDT") {
          if (exchangeId === "binance")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.001,
              timestamp: MOCK_TIMESTAMP,
            };
          if (exchangeId === "bybit")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.0001,
              timestamp: MOCK_TIMESTAMP,
            };
        }
        return null;
      }
    );

    const opportunities = await opportunityService.findOpportunities(
      exchangeIds,
      rawPairs,
      threshold
    );

    expect(opportunities).toHaveLength(0);
    expect(mockGetFundingRate).toHaveBeenCalledTimes(2);
  });

  it("should handle multiple pairs and exchanges, finding multiple opportunities", async () => {
    // Explicit mock for this test
    mockGetTradingFees.mockImplementation(
      async (
        exchangeId: string,
        symbol: string
      ): Promise<ccxt.TradingFeeInterface | null> => {
        if (symbol === "BTC/USDT") {
          if (exchangeId === "binance")
            return {
              symbol,
              taker: 0.0001,
              maker: 0.0001,
              percentage: true,
              tierBased: false,
              info: {},
            };
          if (exchangeId === "bybit")
            return {
              symbol,
              taker: 0.0001,
              maker: 0.0001,
              percentage: true,
              tierBased: false,
              info: {},
            };
        }
        if (symbol === "ETH/USDT") {
          if (exchangeId === "binance")
            return {
              symbol,
              taker: 0.0001,
              maker: 0.0001,
              percentage: true,
              tierBased: false,
              info: {},
            };
          if (exchangeId === "bybit")
            return {
              symbol,
              taker: 0.0001,
              maker: 0.0001,
              percentage: true,
              tierBased: false,
              info: {},
            };
        }
        if (symbol === "ADA/USDT") {
          if (exchangeId === "okx")
            return {
              symbol,
              taker: 0.0,
              maker: 0.0,
              percentage: true,
              tierBased: false,
              info: {},
            };
          if (exchangeId === "kraken")
            return {
              symbol,
              taker: 0.0,
              maker: 0.0,
              percentage: true,
              tierBased: false,
              info: {},
            };
        }
        return null;
      }
    );

    const exchangeIds: ExchangeId[] = ["binance", "bybit", "okx", "kraken"];
    const rawPairs: TradingPairSymbol[] = ["BTC/USDT", "ETH/USDT", "ADA/USDT"];
    const threshold = 0.0005;

    mockGetFundingRate.mockImplementation(
      async (
        exchangeIdInput: string,
        symbol: string
      ): Promise<FundingRateInfo | null> => {
        const exchangeId = exchangeIdInput as ExchangeId;
        if (symbol === "BTC/USDT") {
          if (exchangeId === "binance")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.001,
              timestamp: MOCK_TIMESTAMP,
            };
          if (exchangeId === "bybit")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.0001,
              timestamp: MOCK_TIMESTAMP,
            };
        }
        if (symbol === "ETH/USDT") {
          if (exchangeId === "binance")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.001,
              timestamp: MOCK_TIMESTAMP,
            };
          if (exchangeId === "bybit")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.0001,
              timestamp: MOCK_TIMESTAMP,
            };
        }
        if (symbol === "ADA/USDT") {
          if (exchangeId === "okx")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.0005,
              timestamp: MOCK_TIMESTAMP,
            };
          if (exchangeId === "kraken")
            return {
              exchange: exchangeId,
              pair: symbol,
              fundingRate: 0.001,
              timestamp: MOCK_TIMESTAMP,
            };
        }
        return null;
      }
    );

    const opportunities = await opportunityService.findOpportunities(
      exchangeIds,
      rawPairs,
      threshold
    );

    expect(mockGetFundingRate).toHaveBeenCalledTimes(12);
    expect(opportunities).toHaveLength(3);

    const btcOpp = opportunities.find((opp) => opp.pair === "BTC/USDT");
    expect(btcOpp).toBeDefined();
    expect(btcOpp).toMatchObject({
      pair: "BTC/USDT",
      longExchange: "bybit",
      shortExchange: "binance",
      longRate: 0.0001,
      shortRate: 0.001,
      rateDifference: 0.0009,
      longExchangeTakerFeeRate: 0.0001,
      shortExchangeTakerFeeRate: 0.0001,
      totalEstimatedFees: 0.0002,
      netRateDifference: 0.0007,
      timestamp: MOCK_TIMESTAMP,
    });

    const ethOpp = opportunities.find(
      (opp) =>
        opp.pair === "ETH/USDT" &&
        opp.longExchange === "bybit" &&
        opp.shortExchange === "binance"
    );
    expect(ethOpp).toBeDefined();
    expect(ethOpp?.pair).toBe("ETH/USDT");
    expect(ethOpp?.longExchange).toBe("bybit");
    expect(ethOpp?.shortExchange).toBe("binance");
    expect(ethOpp?.longRate).toBe(0.0001);
    expect(ethOpp?.shortRate).toBe(0.001);
    expect(ethOpp?.rateDifference).toBeCloseTo(0.0009);
    expect(ethOpp?.netRateDifference).toBeCloseTo(0.0007);
    expect(ethOpp?.timestamp).toEqual(expect.any(Number));

    const adaOpp = opportunities.find((opp) => opp.pair === "ADA/USDT");
    expect(adaOpp).toBeDefined();
    expect(adaOpp).toMatchObject({
      pair: "ADA/USDT",
      longExchange: "okx",
      shortExchange: "kraken",
      longRate: 0.0005,
      shortRate: 0.001,
      rateDifference: 0.0005,
      longExchangeTakerFeeRate: 0.0,
      shortExchangeTakerFeeRate: 0.0,
      totalEstimatedFees: 0.0,
      netRateDifference: 0.0005,
      timestamp: MOCK_TIMESTAMP,
    });
  });

  it("should correctly ignore pairs where one or more exchanges return null rates", async () => {
    const exchangeIds: ExchangeId[] = ["binance", "bybit"];
    const rawPairs: TradingPairSymbol[] = ["BTC/USDT"];
    const threshold = 0.0005;

    mockGetFundingRate.mockImplementation(
      async (
        exchangeIdInput: string,
        symbol: string
      ): Promise<FundingRateInfo | null> => {
        const exchangeId = exchangeIdInput as ExchangeId;
        if (symbol !== "BTC/USDT") return null;
        if (exchangeId === "binance") {
          return {
            exchange: exchangeId,
            pair: symbol,
            timestamp: MOCK_TIMESTAMP,
            fundingRate: 0.001,
          };
        }
        if (exchangeId === "bybit") {
          return null;
        }
        return null;
      }
    );

    const opportunities = await opportunityService.findOpportunities(
      exchangeIds,
      rawPairs,
      threshold
    );

    expect(opportunities).toHaveLength(0);
    expect(mockGetFundingRate).toHaveBeenCalledTimes(2);
  });

  it("should log an error if sending Telegram notification fails", async () => {
    vi.clearAllMocks(); // Maximum isolation: Clear all mocks at the start of THIS test

    // const mockTelegramConfigForBadService: TelegramConfig = {
    //   botToken: "test-bad-token",
    //   chatId: "test-bad-chat",
    //   logger: mockLogger, // Use the existing fullyMockedLogger
    // };

    // Create an object that only has the public methods of TelegramService that OpportunityService might call,
    // and ensure they are Vitest mocks.
    const mockBadTelegramServiceMethodsOnly = {
      getBotInstance: vi.fn(), // Add .mockReturnValue({} as Bot) if its return is used
      sendOpportunityNotification: vi
        .fn()
        .mockRejectedValue(new Error("Telegram Boom")),
      sendMessage: vi.fn().mockResolvedValue(undefined),
      stop: vi.fn().mockResolvedValue(undefined),
      // DO NOT include private members or methods not on TelegramService here
    };

    // Cast this object to Mocked<TelegramService> via unknown.
    // This acknowledges that the object doesn't perfectly match TelegramService's full shape (e.g. private members)
    // but is sufficient for testing the interaction with its public mocked methods.
    const mockBadTelegramServiceInstance =
      mockBadTelegramServiceMethodsOnly as unknown as Mocked<TelegramService>;

    // Re-establish the mockExchangeService for this test after vi.clearAllMocks()
    // as clearAllMocks would have cleared its internal mocks too.
    // Ensure it points to the global mock functions initially, which we will then override.
    mockExchangeService.getFundingRate = mockGetFundingRate;
    mockExchangeService.getTradingFees = mockGetTradingFees;
    mockExchangeService.getExchangeInstance = mockGetCcxtInstance; // Point to the global one first

    // Specific mock setup for this test to ensure an opportunity IS found
    mockGetFundingRate.mockImplementation(async (id, p) => {
      const exId = id as ExchangeId;
      const pairSymbol = p as TradingPairSymbol;
      if (pairSymbol === "BTC/USDT") {
        if (exId === "exchangeA")
          return createFundingRateInfo(exId, pairSymbol, 0.0005);
        if (exId === "exchangeB")
          return createFundingRateInfo(exId, pairSymbol, 0.0001);
      }
      return null;
    });

    mockGetTradingFees.mockImplementation(async (id, p) => {
      const exId = id as ExchangeId;
      const pairSymbol = p as TradingPairSymbol;
      if (pairSymbol === "BTC/USDT") {
        if (exId === "exchangeA") return createTradingFee(pairSymbol, 0.00005);
        if (exId === "exchangeB") return createTradingFee(pairSymbol, 0.00005);
      }
      return null;
    });

    // Mock getExchangeInstance to control general fee status
    mockExchangeService.getExchangeInstance = vi
      .fn() // Use the direct mockExchangeService instance
      .mockImplementation(async (exchangeId: ExchangeId) => {
        if (exchangeId === "exchangeA" || exchangeId === "exchangeB") {
          return createMockCcxtExchange(0.00005);
        }
        return createMockCcxtExchange(0.001);
      });

    const configWithBadTelegram: OpportunityServiceConfig = {
      exchangeService: mockExchangeService,
      telegramService: mockBadTelegramServiceInstance,
      logger: mockLogger,
      monitoredPairs: [
        { symbol: "BTC/USDT", base: "BTC", quote: "USDT", type: "swap" },
      ],
      exchanges: ["exchangeA" as ExchangeId, "exchangeB" as ExchangeId],
      threshold: DEFAULT_THRESHOLD,
    };
    opportunityService = new OpportunityService(configWithBadTelegram);

    const opportunities = await opportunityService.findOpportunities(
      ["exchangeA" as ExchangeId, "exchangeB" as ExchangeId],
      ["BTC/USDT"],
      DEFAULT_THRESHOLD
    );
    expect(opportunities.length).toBeGreaterThan(0);

    expect(
      mockBadTelegramServiceInstance.sendOpportunityNotification
    ).toHaveBeenCalledTimes(1);

    let telegramErrorLogged = false;
    for (const call of mockLogger.error.mock.calls) {
      const message = call[0] as string;
      const loggedMeta = call[1];

      if (
        message === "Failed to send Telegram notification" &&
        typeof loggedMeta === "object" &&
        loggedMeta !== null &&
        "teleError" in loggedMeta // Narrows type of loggedMeta to include teleError
      ) {
        // TypeScript infers loggedMeta has teleError. Its value is initially unknown.
        const errorValue = loggedMeta.teleError;
        if (
          errorValue instanceof Error &&
          errorValue.message === "Telegram Boom"
        ) {
          telegramErrorLogged = true;
          break;
        }
      }
    }
    expect(telegramErrorLogged).toBe(true);
  });

  describe("findOpportunities - Stricter Fee Handling", () => {
    const PAIR_DEFAULT = "BTC/USDT";
    const EX_A = "exchangeA" as ExchangeId;
    const EX_B = "exchangeB" as ExchangeId;
    const EX_C = "exchangeC" as ExchangeId;
    const THRESHOLD = 0.001; // 0.1%

    beforeEach(() => {
      fullyMockedLogger.debug.mockClear();
      fullyMockedLogger.info.mockClear();
      fullyMockedLogger.error.mockClear();
      mockGetFundingRate.mockReset().mockResolvedValue(null);
      mockGetTradingFees.mockReset().mockResolvedValue(null);
      mockGetCcxtInstance
        .mockReset()
        .mockResolvedValue(createMockCcxtExchange(0.001));
    });

    it("Test Case 1: Fees present, opportunity found", async () => {
      mockGetFundingRate.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_DEFAULT) return null;
        if (exchangeId === EX_A)
          return createFundingRateInfo(EX_A, PAIR_DEFAULT, 0.0001);
        if (exchangeId === EX_B)
          return createFundingRateInfo(EX_B, PAIR_DEFAULT, 0.0025);
        return null;
      });
      mockGetTradingFees.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_DEFAULT) return null;
        if (exchangeId === EX_A) return createTradingFee(PAIR_DEFAULT, 0.0001);
        if (exchangeId === EX_B) return createTradingFee(PAIR_DEFAULT, 0.0001);
        return null;
      });
      const newOpportunities = await opportunityService.findOpportunities(
        [EX_A, EX_B],
        [PAIR_DEFAULT],
        THRESHOLD
      );
      expect(newOpportunities.length).toBe(1);
      expect(newOpportunities[0].longExchange).toBe(EX_A);
      expect(newOpportunities[0].shortExchange).toBe(EX_B);
      expect(newOpportunities[0].longRate).toBe(0.0001);
      expect(newOpportunities[0].shortRate).toBe(0.0025);
      expect(newOpportunities[0].longExchangeTakerFeeRate).toBe(0.0001);
      expect(newOpportunities[0].shortExchangeTakerFeeRate).toBe(0.0001);
      expect(newOpportunities[0].totalEstimatedFees).toBe(0.0002);
      expect(newOpportunities[0].netRateDifference).toBeCloseTo(0.0022);
    });

    it("Test Case 2: One exchange generally fee-free, other specific fee, opportunity", async () => {
      const PAIR_ETH = "ETH/USDT";
      mockGetFundingRate.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_ETH) return null;
        if (exchangeId === EX_A)
          return createFundingRateInfo(EX_A, PAIR_ETH, 0.0002);
        if (exchangeId === EX_B)
          return createFundingRateInfo(EX_B, PAIR_ETH, 0.002);
        return null;
      });
      mockGetTradingFees.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_ETH) return null;
        if (exchangeId === EX_A) {
          mockLogger.info("TC2: EX_A getTradingFees returns null");
          return null;
        }
        if (exchangeId === EX_B) {
          mockLogger.info("TC2: EX_B getTradingFees returns fee object");
          return createTradingFee(PAIR_ETH, 0.0005);
        }
        return null;
      });
      mockGetCcxtInstance.mockImplementation(async (exchangeId) => {
        if (exchangeId === EX_A) {
          mockLogger.info("TC2: EX_A getExchangeInstance (fee-free)");
          return createMockCcxtExchange(undefined, true);
        }
        if (exchangeId === EX_B) {
          mockLogger.info("TC2: EX_B getExchangeInstance (not fee-free)");
          return createMockCcxtExchange(0.001);
        }
        return createMockCcxtExchange(0.001);
      });

      const opportunities = await opportunityService.findOpportunities(
        [EX_A, EX_B],
        [PAIR_ETH],
        THRESHOLD
      );

      expect(opportunities.length).toBe(1);
      expect(opportunities[0].longExchange).toBe(EX_A);
      expect(opportunities[0].shortExchange).toBe(EX_B);
      expect(opportunities[0].longRate).toBe(0.0002);
      expect(opportunities[0].shortRate).toBe(0.002);
      expect(opportunities[0].longExchangeTakerFeeRate).toBe(0);
      expect(opportunities[0].shortExchangeTakerFeeRate).toBe(0.0005);
      expect(opportunities[0].totalEstimatedFees).toBe(0.0005);
      expect(opportunities[0].netRateDifference).toBeCloseTo(0.0013);
    });

    it("Test Case 3: One exchange no specific fee & NOT generally fee-free, excluded", async () => {
      const PAIR_ADA = "ADA/USDT";
      mockGetFundingRate.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_ADA) return null;
        if (exchangeId === EX_A)
          return createFundingRateInfo(EX_A, PAIR_ADA, 0.0003);
        if (exchangeId === EX_B)
          return createFundingRateInfo(EX_B, PAIR_ADA, 0.0007);
        return null;
      });
      mockGetTradingFees.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_ADA) return null;
        if (exchangeId === EX_A) return null;
        if (exchangeId === EX_B) return createTradingFee(PAIR_ADA, 0.001);
        return null;
      });
      mockGetCcxtInstance.mockImplementation(async (exchangeId) => {
        if (exchangeId === EX_A) return createMockCcxtExchange(0.001);
        if (exchangeId === EX_B) return createMockCcxtExchange(0.001);
        return createMockCcxtExchange(0.001);
      });

      await opportunityService.findOpportunities(
        [EX_A, EX_B],
        [PAIR_ADA],
        THRESHOLD
      );
      const foundLog = mockLogger.debug.mock.calls.some(
        (call) =>
          typeof call[0] === "string" &&
          call[0].includes(
            `Excluding ${EX_A} for pair ${PAIR_ADA}: Funding rate present but no specific fee info and not generally fee-free.`
          )
      );
      expect(foundLog).toBe(true);
    });

    it("Test Case 4: Fees make opportunity unprofitable", async () => {
      const PAIR_SOL = "SOL/USDT";
      mockGetFundingRate.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_SOL) return null;
        if (exchangeId === EX_A)
          return createFundingRateInfo(EX_A, PAIR_SOL, 0.0001);
        if (exchangeId === EX_B)
          return createFundingRateInfo(EX_B, PAIR_SOL, 0.0003);
        return null;
      });
      mockGetTradingFees.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_SOL) return null;
        if (exchangeId === EX_A) return createTradingFee(PAIR_SOL, 0.002);
        if (exchangeId === EX_B) return createTradingFee(PAIR_SOL, 0.002);
        return null;
      });
      const opportunities = await opportunityService.findOpportunities(
        [EX_A, EX_B],
        [PAIR_SOL],
        THRESHOLD
      );
      expect(opportunities.length).toBe(0);
    });

    it("Test Case 5: Exchange missing funding rate, excluded", async () => {
      const PAIR_DOT = "DOT/USDT";
      mockGetFundingRate.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_DOT) return null;
        if (exchangeId === EX_A) return null;
        if (exchangeId === EX_B)
          return createFundingRateInfo(EX_B, PAIR_DOT, 0.0005);
        return null;
      });
      mockGetTradingFees.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_DOT) return null;
        if (exchangeId === EX_A) return createTradingFee(PAIR_DOT, 0.001);
        if (exchangeId === EX_B) return createTradingFee(PAIR_DOT, 0.001);
        return null;
      });

      await opportunityService.findOpportunities(
        [EX_A, EX_B],
        [PAIR_DOT],
        THRESHOLD
      );
      // Check for the warn log when unique rates are less than 2
      const foundWarnLog = mockLogger.warn.mock.calls.some(
        (call) =>
          typeof call[0] === "string" &&
          call[0].includes(
            `Skipping ${PAIR_DOT} as it does not have at least two different funding rates`
          )
      );
      // Additionally, check that the debug log (which was previously expected) is NOT there
      const foundDebugLog = mockLogger.debug.mock.calls.some(
        (call) =>
          typeof call[0] === "string" &&
          call[0].includes(
            `Skipping ${PAIR_DOT} - not enough exchanges with funding rates.`
          )
      );
      expect(foundWarnLog).toBe(true); // This should now pass
      expect(foundDebugLog).toBe(false); // This confirms the other path wasn't taken
    });

    it("Test Case 6: Complex 3-exchange scenario", async () => {
      const PAIR_LINK = "LINK/USDT";
      mockGetFundingRate.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_LINK) return null;
        if (exchangeId === EX_A)
          return createFundingRateInfo(EX_A, PAIR_LINK, 0.0001);
        if (exchangeId === EX_B)
          return createFundingRateInfo(EX_B, PAIR_LINK, 0.0008);
        if (exchangeId === EX_C)
          return createFundingRateInfo(EX_C, PAIR_LINK, 0.0003);
        return null;
      });
      mockGetTradingFees.mockImplementation(async (exchangeId, pair) => {
        if (pair !== PAIR_LINK) return null;
        if (exchangeId === EX_A) return createTradingFee(PAIR_LINK, 0.0005);
        if (exchangeId === EX_B) return null;
        if (exchangeId === EX_C) return null;
        return null;
      });
      mockGetCcxtInstance.mockImplementation(async (exchangeId) => {
        if (exchangeId === EX_A) {
          mockLogger.info("TC6: EX_A getExchangeInstance (not fee-free)");
          return createMockCcxtExchange(0.0005);
        }
        if (exchangeId === EX_B) {
          mockLogger.info("TC6: EX_B getExchangeInstance (FEE-FREE)");
          return createMockCcxtExchange(undefined, true);
        }
        if (exchangeId === EX_C) {
          mockLogger.info("TC6: EX_C getExchangeInstance (not fee-free)");
          return createMockCcxtExchange(0.001);
        }
        return createMockCcxtExchange(0.001);
      });

      const localThreshold = 0.00015;
      const opportunities = await opportunityService.findOpportunities(
        [EX_A, EX_B, EX_C],
        [PAIR_LINK],
        localThreshold
      );

      const foundExCLog = mockLogger.debug.mock.calls.some(
        (call) =>
          typeof call[0] === "string" &&
          call[0].includes(
            `Excluding ${EX_C} for pair ${PAIR_LINK}: Funding rate present but no specific fee info and not generally fee-free.`
          )
      );
      expect(foundExCLog).toBe(true);

      const foundExBExclusionLog = mockLogger.debug.mock.calls.some(
        (call) =>
          typeof call[0] === "string" &&
          call[0].includes(
            `Excluding ${EX_B} for pair ${PAIR_LINK}: Funding rate present but no specific fee info and not generally fee-free.`
          )
      );
      expect(foundExBExclusionLog).toBe(false);

      expect(opportunities.length).toBe(1);
      const op = opportunities[0];
      expect(op.longExchange).toBe(EX_A);
      expect(op.shortExchange).toBe(EX_B);
      expect(op.longRate).toBe(0.0001);
      expect(op.shortRate).toBe(0.0008);
      expect(op.longExchangeTakerFeeRate).toBe(0.0005);
      expect(op.shortExchangeTakerFeeRate).toBe(0);
      expect(op.totalEstimatedFees).toBe(0.0005);
      expect(op.netRateDifference).toBeCloseTo(0.0002);
    });
  });
});

afterAll(() => {
  vi.doUnmock("ccxt");
  vi.resetModules();
});

const createFundingRateInfo = (
  exchange: ExchangeId,
  pair: TradingPairSymbol,
  fundingRate: number,
  timestamp: number = MOCK_TIMESTAMP
): FundingRateInfo => ({
  exchange,
  pair,
  fundingRate,
  timestamp,
});

const createTradingFee = (
  symbol: TradingPairSymbol,
  taker: number,
  maker: number = taker
): ccxt.TradingFeeInterface => ({
  symbol,
  taker,
  maker,
  percentage: true,
  tierBased: false,
  info: {},
});

interface ExactFeeStructure {
  tierBased: boolean;
  percentage: boolean;
  taker: number;
  maker: number;
}

const createMockCcxtExchange = (
  takerFeeParam: number | undefined,
  isFeeFreeOverride?: boolean
): Partial<ccxt.Exchange> => {
  let tradingFeeConfig: ExactFeeStructure | undefined;

  if (isFeeFreeOverride === true) {
    tradingFeeConfig = {
      taker: 0,
      maker: 0,
      percentage: true,
      tierBased: false,
    };
  } else if (takerFeeParam !== undefined) {
    tradingFeeConfig = {
      taker: takerFeeParam,
      maker: takerFeeParam,
      percentage: true,
      tierBased: false,
    };
  }

  const fundingFeeConfig = {
    tierBased: false,
    percentage: false,
    withdraw: {},
    deposit: {},
  };

  return {
    fees: tradingFeeConfig
      ? {
          trading: tradingFeeConfig,
          funding: fundingFeeConfig,
        }
      : undefined,
    has: {
      fetchFundingRate: true,
      fetchTradingFees: true,
    },
  };
};

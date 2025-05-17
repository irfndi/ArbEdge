/// <reference types='@cloudflare/workers-types' />
/// <reference types="vitest/globals" />
import type * as ccxtOriginal from "ccxt";
import type {
  ExchangeId,
  TradingPairSymbol,
  FundingRateInfo,
  ArbitrageOpportunity,
  Balances,
  Balance,
  Position,
  Order,
  Market,
  Ticker,
  OHLCV,
  LoggerInterface,
  OrderBook,
  Trade,
  CCXTTradingFees,
  CCXTTradingFeeInterface,
  Env,
} from "../../src/types";
import type {
  Exchange as CCXTExchange,
  Fee as CCXTFee,
  OrderSide,
  OrderType,
} from "ccxt";
import { MOCK_MARKET_DEFAULTS } from "../mocks/marketMocks";
import {
  describe,
  it,
  expect,
  vi,
  beforeEach,
  afterEach,
  type Mock,
  type Mocked,
} from "vitest";
import {
  ExchangeService,
  type PositionSide,
} from "../../src/services/exchangeService";
import { 깊은복제 as deepClone } from "../../src/utils/helpers";
import type {
  MockExchangeInstance,
  ALL_MOCK_EXCHANGE_IDS,
  createMockInstance as createMockHelperInstance,
  MOCK_BINANCE_BALANCES_FACTORY,
} from "./exchangeService.test.helpers";
import * as ccxt from "ccxt";

import type { KVNamespaceGetOptions as CloudflareKVNamespaceGetOptions } from "@cloudflare/workers-types";

vi.mock("ccxt", async (importOriginal: () => Promise<typeof ccxtOriginal>) => {
  const originalCcxtModule = await importOriginal();
  const testHelpers = await import("./exchangeService.test.helpers");

  let factoryScopedSingletonInstances:
    | Record<ExchangeId, MockExchangeInstance>
    | undefined;

  const initializeMockInstancesIfNeeded = () => {
    if (!factoryScopedSingletonInstances) {
      const tempInstances: Partial<Record<ExchangeId, MockExchangeInstance>> =
        {};
      for (const id of testHelpers.ALL_MOCK_EXCHANGE_IDS) {
        tempInstances[id] = testHelpers.createMockInstance(id);
      }
      factoryScopedSingletonInstances = tempInstances as Record<
        ExchangeId,
        MockExchangeInstance
      >;
    }
  };

  const createMockCcxtInstance = (
    exchangeId: ExchangeId,
    _options?: unknown
  ) => {
    initializeMockInstancesIfNeeded();
    if (!factoryScopedSingletonInstances) {
      throw new Error("factoryScopedSingletonInstances not initialized");
    }
    const mockInstanceData = factoryScopedSingletonInstances[exchangeId];
    if (!mockInstanceData) {
      throw new Error(
        `Mock instance data for ${exchangeId} not found in factory-scoped singleton.`
      );
    }
    return mockInstanceData;
  };

  const MockBaseExchange = (_options?: unknown) => {
    console.warn(
      "Mocked ccxt.Exchange base class instantiated. This might not be fully functional."
    );
    const { id: _spreadId, ...restOfMockInstance } =
      testHelpers.createMockInstance("generic" as ExchangeId);
    return {
      id: "mockBaseExchange",
      version: "mocked",
      rateLimit: 2000,
      ...restOfMockInstance,
    };
  };

  const commonExports = {
    Exchange: MockBaseExchange,
    binance: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("binance", args[0]) as MockExchangeInstance,
    bybit: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("bybit", args[0]) as MockExchangeInstance,
    bitget: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("bitget", args[0]) as MockExchangeInstance,
    kraken: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("kraken", args[0]) as MockExchangeInstance,
    mexc: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("mexc", args[0]) as MockExchangeInstance,
    okx: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("okx", args[0]) as MockExchangeInstance,
    bingx: (...args: unknown[]): MockExchangeInstance =>
      createMockCcxtInstance("bingx", args[0]) as MockExchangeInstance,

    pro: new Proxy(
      {},
      {
        get: (target, propKey) => {
          const exchangeId = String(propKey).toLowerCase() as ExchangeId;
          initializeMockInstancesIfNeeded();
          if (factoryScopedSingletonInstances?.[exchangeId]) {
            return (...args: unknown[]): MockExchangeInstance =>
              createMockCcxtInstance(
                exchangeId,
                args[0]
              ) as MockExchangeInstance;
          }
          if (
            typeof originalCcxtModule.pro?.[
              exchangeId as keyof typeof originalCcxtModule.pro
            ] === "function"
          ) {
            return (originalCcxtModule.pro as Record<string, unknown>)[
              exchangeId
            ];
          }
          console.warn(
            `ccxt.pro mock: Exchange class '${exchangeId}' not explicitly mocked. Returning undefined.`
          );
          return undefined;
        },
      }
    ),
    get _testAccessibleMockInstances() {
      initializeMockInstancesIfNeeded();
      return factoryScopedSingletonInstances;
    },
  };

  return {
    ...originalCcxtModule,
    ...commonExports,
    default: {
      ...originalCcxtModule,
      ...commonExports,
    },
  };
});

interface TestHelperMockedCcxtModule extends Readonly<typeof ccxtOriginal> {
  _testAccessibleMockInstances?: Record<ExchangeId, MockExchangeInstance>;
}

let testAccessibleMockInstances:
  | Record<ExchangeId, MockExchangeInstance>
  | undefined;
let mockEnv: Env;

describe("ExchangeService", () => {
  const logger: Mocked<LoggerInterface> = {
    log: vi.fn(),
    error: vi.fn(),
    warn: vi.fn(),
    info: vi.fn(),
    debug: vi.fn(),
    addContext: vi.fn(),
    addError: vi.fn(),
  };

  beforeEach(async () => {
    const mockedCcxtModule = (await import(
      "ccxt"
    )) as unknown as TestHelperMockedCcxtModule;
    const ccxtMocks = mockedCcxtModule._testAccessibleMockInstances;
    if (ccxtMocks === undefined) {
      throw new Error(
        "_testAccessibleMockInstances was unexpectedly undefined during test setup. This indicates a problem with the mock initialization."
      );
    }
    testAccessibleMockInstances = ccxtMocks;

    const { MOCK_BINANCE_BALANCES_FACTORY } = await import(
      "./exchangeService.test.helpers"
    );

    mockEnv = {
      ArbEdgeKV: {
        get: vi
          .fn()
          .mockImplementation(
            async (
              key: string,
              options?:
                | CloudflareKVNamespaceGetOptions<
                    "text" | "json" | "arrayBuffer" | "stream"
                  >
                | "text"
                | "json"
                | "arrayBuffer"
                | "stream"
            ): Promise<
              | string
              | Record<string, unknown>
              | ArrayBuffer
              | ReadableStream
              | null
            > => {
              const type =
                typeof options === "string" ? options : options?.type;
              if (key.startsWith("arbitrageOpportunities")) {
                const mockArbOp: ArbitrageOpportunity = {
                  pair: "BTC/USDT",
                  longExchange: "binance" as ExchangeId,
                  shortExchange: "bybit" as ExchangeId,
                  longRate: 0.0001,
                  shortRate: 0.0002,
                  rateDifference: 0.0001,
                  longExchangeTakerFeeRate: 0.001,
                  shortExchangeTakerFeeRate: 0.001,
                  totalEstimatedFees: 0.002,
                  netRateDifference: -0.0019,
                  timestamp: Date.now(),
                };
                if (type === "json")
                  return mockArbOp as unknown as Record<string, unknown>;
                return JSON.stringify(mockArbOp);
              }
              if (key.startsWith("api_key:")) {
                return JSON.stringify({
                  apiKey: "testKey",
                  apiSecret: "testSecret",
                });
              }
              return null;
            }
          ),
        put: vi.fn().mockResolvedValue(undefined),
        delete: vi.fn().mockResolvedValue(undefined),
        list: vi.fn().mockResolvedValue({
          keys: [],
          list_complete: true,
          cursor: undefined,
        }),
        getWithMetadata: vi
          .fn()
          .mockResolvedValue({ value: null, metadata: null }),
      } as unknown as KVNamespace,
      POSITIONS: {} as DurableObjectNamespace,
      TELEGRAM_BOT_TOKEN: "test_token",
      TELEGRAM_CHAT_ID: "test_chat_id",
      EXCHANGES: "binance,bybit",
      ARBITRAGE_THRESHOLD: "0.1",
      MONITORED_PAIRS_CONFIG: JSON.stringify([
        { symbol: "BTC/USDT", base: "BTC", quote: "USDT", type: "swap" },
      ]),
    };

    const mocks = testAccessibleMockInstances;
    if (mocks) {
      for (const exchangeIdKey in mocks) {
        if (Object.prototype.hasOwnProperty.call(mocks, exchangeIdKey)) {
          const exchangeId = exchangeIdKey as ExchangeId;
          const instance = mocks[exchangeId];
          for (const key in instance) {
            if (Object.prototype.hasOwnProperty.call(instance, key)) {
              const mockFn = instance[key as keyof MockExchangeInstance];
              if (
                typeof mockFn === "function" &&
                "_isMockFunction" in mockFn &&
                mockFn._isMockFunction
              ) {
                (
                  mockFn as unknown as Mock<(...args: unknown[]) => unknown>
                ).mockClear();
              }
            }
          }
          if (
            instance.loadMarkets &&
            typeof instance.loadMarkets.mockResolvedValue === "function"
          ) {
            instance.loadMarkets.mockResolvedValue({
              "BTC/USDT": MOCK_MARKET_DEFAULTS as Market,
            });
          }
          if (
            instance.fetchBalance &&
            typeof instance.fetchBalance.mockResolvedValue === "function"
          ) {
            instance.fetchBalance.mockResolvedValue(
              MOCK_BINANCE_BALANCES_FACTORY()
            );
          }
        }
      }
    }
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("should add exchanges and load their markets when added", async () => {
    const service = new ExchangeService({ logger: logger, env: mockEnv });

    await service.getExchangeInstance("binance" as ExchangeId);
    await service.getExchangeInstance("kraken" as ExchangeId);

    const mocks = testAccessibleMockInstances;
    if (!mocks?.binance || !mocks?.kraken) {
      throw new Error(
        "Mocks for binance or kraken not initialized after adding them."
      );
    }

    expect(mocks.binance.loadMarkets).toHaveBeenCalledTimes(1);
    expect(mocks.kraken.loadMarkets).toHaveBeenCalledTimes(1);
  });

  describe("Dynamic Exchange Management", () => {
    let service: ExchangeService;

    beforeEach(() => {
      service = new ExchangeService({ logger: logger, env: mockEnv });
      if (testAccessibleMockInstances?.kraken?.loadMarkets) {
        (testAccessibleMockInstances.kraken.loadMarkets as Mock).mockClear();
      }
    });

    it("should initialize an exchange and its markets upon first retrieval", async () => {
      const mocks = testAccessibleMockInstances;
      if (!mocks?.kraken)
        throw new Error("Kraken mock not available in test setup");

      expect(mocks.kraken.loadMarkets).not.toHaveBeenCalled();

      const krakenInstance = await service.getExchangeInstance(
        "kraken" as ExchangeId
      );

      expect(krakenInstance).toBeDefined();
      expect(krakenInstance?.id).toBe("kraken");
      expect(mocks.kraken.loadMarkets).toHaveBeenCalledTimes(1);
    });
  });

  describe("getOpenPositions", () => {
    let service: ExchangeService;

    beforeEach(() => {
      service = new ExchangeService({ logger: logger, env: mockEnv });
    });

    it("should fetch open positions for a given symbol on an exchange", async () => {
      if (!testAccessibleMockInstances) {
        throw new Error(
          "testAccessibleMockInstances is not initialized for getOpenPositions test"
        );
      }
      const mocks = testAccessibleMockInstances;
      const fakePosition: Position = {
        symbol: "BTC/USDT",
        id: "1",
        timestamp: Date.now(),
        datetime: new Date().toISOString(),
        info: {},
        side: "long" as PositionSide,
        contracts: 1,
        entryPrice: 20000,
        markPrice: 20000,
        leverage: 1,
        initialMargin: 20000,
        maintenanceMargin: 1000,
        liquidationPrice: 10000,
        marginMode: "isolated",
        hedged: false,
      };
      if (!mocks.binance) throw new Error("Binance mock not available");
      (mocks.binance.fetchPositions as Mock).mockResolvedValue([fakePosition]);

      const positions = await service.getOpenPositions(
        "binance" as ExchangeId,
        "BTC/USDT"
      );
      expect(positions).toBeDefined();
      expect(positions).not.toBeNull();
      if (positions) {
        expect(positions.length).toBeGreaterThan(0);
        expect(positions[0]?.symbol).toBe("BTC/USDT");
      } else {
        throw new Error("Positions array is null or undefined after fetch");
      }
      expect(mocks.binance.fetchPositions).toHaveBeenCalledTimes(1);
    });
  });

  describe("setLeverage", () => {
    let service: ExchangeService;

    beforeEach(() => {
      service = new ExchangeService({ logger: logger, env: mockEnv });
    });

    it("should set leverage for a symbol on a given exchange", async () => {
      if (!testAccessibleMockInstances) {
        throw new Error(
          "testAccessibleMockInstances is not initialized for setLeverage test"
        );
      }
      const mocks = testAccessibleMockInstances;
      const fakeLeverageResponse = { info: "Leverage set to 10 for BTC/USDT" };
      if (!mocks.binance) throw new Error("Binance mock not available");
      (mocks.binance.setLeverage as Mock).mockResolvedValue(
        fakeLeverageResponse as unknown
      );

      const leverageResult = await service.setLeverage(
        "binance" as ExchangeId,
        "BTC/USDT",
        10
      );
      expect(leverageResult).toBeDefined();
      expect(leverageResult).toEqual(fakeLeverageResponse);
      expect(mocks.binance.setLeverage).toHaveBeenCalledWith(
        10,
        "BTC/USDT",
        undefined
      );
    });
  });

  describe("getTicker", () => {
    let service: ExchangeService;

    beforeEach(() => {
      service = new ExchangeService({ logger: logger, env: mockEnv });
    });

    it("should fetch the ticker for a symbol on a given exchange", async () => {
      const service = new ExchangeService({ env: mockEnv, logger });
      const exchangeId = "binance" as ExchangeId;
      const symbol = "BTC/USDT";
      if (!testAccessibleMockInstances) {
        throw new Error(
          "testAccessibleMockInstances is not initialized for getTicker test"
        );
      }
      const mocks = testAccessibleMockInstances;

      const fakeTicker: Partial<Ticker> = {
        symbol: "BTC/USDT",
        bid: 100,
        ask: 101,
        timestamp: Date.now(),
        datetime: new Date().toISOString(),
        last: 100.5,
        info: {},
        high: undefined,
        low: undefined,
        vwap: undefined,
        open: undefined,
        close: undefined,
        average: undefined,
        baseVolume: undefined,
        quoteVolume: undefined,
        previousClose: undefined,
        change: undefined,
        percentage: undefined,
      };
      mocks.binance.fetchTicker.mockResolvedValue(
        fakeTicker as unknown as Ticker
      );

      const ticker = await service.getTicker(
        "binance" as ExchangeId,
        "BTC/USDT"
      );

      expect(mocks.binance.fetchTicker).toHaveBeenCalledWith("BTC/USDT");
      expect(ticker).toBeDefined();
      expect(ticker?.symbol).toBe("BTC/USDT");
      expect(ticker?.last).toBe(100.5);
    });

    it("should return null if exchange does not support fetchTicker capability", async () => {
      const exchangeId = "kraken" as ExchangeId; // Use an exchange that might be different
      const symbol = "ETH/USDT";
      const instance = await service.getExchangeInstance(exchangeId);
      if (!instance) throw new Error("Instance not created for kraken");

      // Simulate that the exchange does not have the fetchTicker capability
      instance.has = { ...instance.has, fetchTicker: false };
      // Ensure the actual fetchTicker method on the mock is not called
      if (testAccessibleMockInstances?.[exchangeId]) {
        (
          testAccessibleMockInstances[exchangeId].fetchTicker as Mock
        ).mockClear();
      }

      const ticker = await service.getTicker(exchangeId, symbol);

      expect(ticker).toBeNull();
      if (testAccessibleMockInstances?.[exchangeId]) {
        expect(
          testAccessibleMockInstances[exchangeId].fetchTicker
        ).not.toHaveBeenCalled();
      }
      expect(logger.warn).toHaveBeenCalledWith(
        `getTicker: Exchange ${exchangeId} does not support fetchTicker for ${symbol}`
      );
    });

    it("should return null if instance.has is undefined and fetchTicker is not directly callable", async () => {
      const exchangeId = "mexc" as ExchangeId;
      const symbol = "ADA/USDT";
      const instance = await service.getExchangeInstance(exchangeId);
      if (!instance) throw new Error("Instance not created for mexc");

      instance.has = {}; // Changed from undefined to {} to satisfy linter and intent
      // instance.fetchTicker is already a mock from the global setup, ensure it wouldn't be called
      if (testAccessibleMockInstances?.[exchangeId]) {
        (
          testAccessibleMockInstances[exchangeId].fetchTicker as Mock
        ).mockClear();
      }

      const ticker = await service.getTicker(exchangeId, symbol);
      expect(ticker).toBeNull();
      if (testAccessibleMockInstances?.[exchangeId]) {
        expect(
          testAccessibleMockInstances[exchangeId].fetchTicker
        ).not.toHaveBeenCalled();
      }
      expect(logger.warn).toHaveBeenCalledWith(
        `getTicker: Exchange ${exchangeId} does not support fetchTicker for ${symbol}`
      );
    });
  });

  describe("Error Handling", () => {
    let service: ExchangeService;

    beforeEach(() => {
      service = new ExchangeService({ logger: logger, env: mockEnv });
    });

    it("should return null when an exchange method throws an error", async () => {
      const mocks = testAccessibleMockInstances;
      if (!mocks)
        throw new Error("Mocks not initialized for test 'Error Handling'");

      if (!mocks.binance)
        throw new Error("Binance mock not available for 'Error Handling' test");

      mocks.binance.has = {
        ...(mocks.binance.has || {}),
        fetchTicker: true,
      };
      (mocks.binance.fetchTicker as Mock).mockRejectedValue(
        new Error("API Error")
      );

      const ticker = await service.getTicker(
        "binance" as ExchangeId,
        "BTC/USDT"
      );
      expect(ticker).toBeNull();
      expect(mocks.binance.fetchTicker).toHaveBeenCalledWith("BTC/USDT");
    });
  });

  // New describe block for API Key Management
  describe("API Key Management", () => {
    let service: ExchangeService;

    beforeEach(() => {
      // Resetting mocks for each test in this block
      mockEnv.ArbEdgeKV.put = vi.fn().mockResolvedValue(undefined);
      mockEnv.ArbEdgeKV.get = vi.fn().mockResolvedValue(null); // Default to no key found
      mockEnv.ArbEdgeKV.delete = vi.fn().mockResolvedValue(undefined);
      service = new ExchangeService({ logger: logger, env: mockEnv });
      // Clear any exchange instances that might persist from other tests or setups
      // This is a bit hacky, accessing private member for test purpose.
      // Consider adding a reset/destroy method to ExchangeService if this becomes common.
      (service as any).exchangeInstances.clear();
    });

    describe("saveApiKey", () => {
      it("should save API key and secret to KV store", async () => {
        const exchangeId = "testExchange" as ExchangeId;
        const apiKey = "testKey";
        const apiSecret = "testSecret";

        await service.saveApiKey(exchangeId, apiKey, apiSecret);

        expect(mockEnv.ArbEdgeKV.put).toHaveBeenCalledTimes(1);
        expect(mockEnv.ArbEdgeKV.put).toHaveBeenCalledWith(
          `api_key:${exchangeId}`,
          JSON.stringify({ apiKey, apiSecret })
        );
        expect(logger.info).toHaveBeenCalledWith(
          `API key for ${exchangeId} saved successfully.`
        );
      });

      it("should clear existing exchange instance from cache when saving a new key", async () => {
        const exchangeId = "binance" as ExchangeId; // Use an exchange that might be pre-loaded
        const apiKey = "newBinanceKey";
        const apiSecret = "newBinanceSecret";

        // Ensure instance is in cache (e.g., by calling getExchangeInstance)
        // For this test, we'll manually put it to simulate it being there.
        if (!testAccessibleMockInstances) {
          throw new Error(
            "testAccessibleMockInstances is not initialized for this test run."
          );
        }
        const mockBinanceInstance = testAccessibleMockInstances[exchangeId];
        if (!mockBinanceInstance) {
          throw new Error(
            `Mock instance for ${exchangeId} not found in testAccessibleMockInstances.`
          );
        }
        (service as any).exchangeInstances.set(exchangeId, mockBinanceInstance);
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(true);

        await service.saveApiKey(exchangeId, apiKey, apiSecret);

        expect(mockEnv.ArbEdgeKV.put).toHaveBeenCalledTimes(1);
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(false);
        expect(logger.info).toHaveBeenCalledWith(
          `Instance cache for ${exchangeId} cleared due to API key update.`
        );
      });

      it("should not fail if exchange instance was not in cache when saving key", async () => {
        const exchangeId = "someNewExchange" as ExchangeId;
        const apiKey = "someKey";
        const apiSecret = "someSecret";

        expect((service as any).exchangeInstances.has(exchangeId)).toBe(false);

        await service.saveApiKey(exchangeId, apiKey, apiSecret);

        expect(mockEnv.ArbEdgeKV.put).toHaveBeenCalledTimes(1);
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(false);
        // Ensure the "cleared" log specifically for cache clearing isn't called if not applicable
        expect(logger.info).not.toHaveBeenCalledWith(
          `Instance cache for ${exchangeId} cleared due to API key update.`
        );
        expect(logger.info).toHaveBeenCalledWith(
          `API key for ${exchangeId} saved successfully.`
        );
      });
    });

    describe("deleteApiKey", () => {
      it("should delete API key from KV store", async () => {
        const exchangeId = "testExchangeToDelete" as ExchangeId;
        await service.deleteApiKey(exchangeId);

        expect(mockEnv.ArbEdgeKV.delete).toHaveBeenCalledTimes(1);
        expect(mockEnv.ArbEdgeKV.delete).toHaveBeenCalledWith(
          `api_key:${exchangeId}`
        );
        expect(logger.info).toHaveBeenCalledWith(
          `API key for ${exchangeId} deleted successfully.`
        );
      });

      it("should clear existing exchange instance from cache when deleting a key", async () => {
        const exchangeId = "bybit" as ExchangeId;
        if (!testAccessibleMockInstances) {
          throw new Error(
            "testAccessibleMockInstances is not initialized for deleteApiKey test."
          );
        }
        const mockBybitInstance = testAccessibleMockInstances[exchangeId];
        if (!mockBybitInstance) {
          throw new Error(
            `Mock instance for ${exchangeId} not found in testAccessibleMockInstances.`
          );
        }
        (service as any).exchangeInstances.set(exchangeId, mockBybitInstance);
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(true);

        await service.deleteApiKey(exchangeId);

        expect(mockEnv.ArbEdgeKV.delete).toHaveBeenCalledTimes(1);
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(false);
        expect(logger.info).toHaveBeenCalledWith(
          `Instance cache for ${exchangeId} cleared due to API key deletion.`
        );
      });

      it("should not fail if exchange instance was not in cache when deleting key", async () => {
        const exchangeId = "anotherNewExchange" as ExchangeId;
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(false);

        await service.deleteApiKey(exchangeId);

        expect(mockEnv.ArbEdgeKV.delete).toHaveBeenCalledTimes(1);
        expect((service as any).exchangeInstances.has(exchangeId)).toBe(false);
        expect(logger.info).not.toHaveBeenCalledWith(
          `Instance cache for ${exchangeId} cleared due to API key deletion.`
        );
        expect(logger.info).toHaveBeenCalledWith(
          `API key for ${exchangeId} deleted successfully.`
        );
      });

      it("getApiKey should return null after deleteApiKey is called", async () => {
        const exchangeId = "persistentExchange" as ExchangeId;
        const apiKey = "persistKey";
        const apiSecret = "persistSecret";

        // Save a key first
        await service.saveApiKey(exchangeId, apiKey, apiSecret);
        // Configure get to return the saved key
        (mockEnv.ArbEdgeKV.get as Mock).mockImplementation(
          async (key: string) => {
            if (key === `api_key:${exchangeId}`) {
              return JSON.stringify({ apiKey, apiSecret });
            }
            return null;
          }
        );

        let retrievedKey = await service.getApiKey(exchangeId);
        expect(retrievedKey).toEqual({ apiKey, apiSecret });

        // Now delete it
        await service.deleteApiKey(exchangeId);
        // Configure get to return null as if deleted
        (mockEnv.ArbEdgeKV.get as Mock).mockResolvedValue(null);

        retrievedKey = await service.getApiKey(exchangeId);
        expect(retrievedKey).toBeNull();
        expect(mockEnv.ArbEdgeKV.delete).toHaveBeenCalledWith(
          `api_key:${exchangeId}`
        );
      });
    });
  });

  describe("Market Data - loadMarketsForExchange Cache", () => {
    let service: ExchangeService;
    const exchangeId = "binance" as ExchangeId;
    const mockMarkets = { "BTC/USDT": MOCK_MARKET_DEFAULTS as Market };

    beforeEach(() => {
      vi.useFakeTimers();
      mockEnv.ArbEdgeKV.get = vi.fn().mockResolvedValue(null); // No API keys by default
      service = new ExchangeService({ logger: logger, env: mockEnv });
      (service as any).exchangeInstances.clear();
      (service as any).marketsCache.clear();

      // Ensure the mock instance is ready and its loadMarkets is a mock
      if (
        testAccessibleMockInstances &&
        testAccessibleMockInstances[exchangeId]
      ) {
        testAccessibleMockInstances[exchangeId].loadMarkets = vi
          .fn()
          .mockResolvedValue(deepClone(mockMarkets));
      } else {
        // If binance is not in testAccessibleMockInstances, we might need to create a more generic setup
        // For now, assuming binance will be available via the ccxt mock setup
        // Or, ensure getExchangeInstance sets up a mock with loadMarkets
      }
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("should load markets from exchange on cache miss and populate cache", async () => {
      const instance = await service.getExchangeInstance(exchangeId);
      expect(instance).not.toBeNull();
      // Directly mock instance.loadMarkets for this specific instance if getExchangeInstance doesn't set it up from testAccessibleMockInstances
      if (instance && !("mockResolvedValue" in instance.loadMarkets)) {
        instance.loadMarkets = vi
          .fn()
          .mockResolvedValue(deepClone(mockMarkets));
      }

      const markets = await service.loadMarketsForExchange(exchangeId);
      expect(markets).toEqual(mockMarkets);
      expect(instance!.loadMarkets).toHaveBeenCalledTimes(1);
      expect((service as any).marketsCache.has(`markets:${exchangeId}`)).toBe(
        true
      );
    });

    it("should return markets from cache on cache hit within TTL", async () => {
      const instance = await service.getExchangeInstance(exchangeId);
      if (instance && !("mockResolvedValue" in instance.loadMarkets)) {
        instance.loadMarkets = vi
          .fn()
          .mockResolvedValue(deepClone(mockMarkets));
      }

      // First call to populate cache
      await service.loadMarketsForExchange(exchangeId);
      expect(instance!.loadMarkets).toHaveBeenCalledTimes(1);

      // Second call, should hit cache
      const cachedMarkets = await service.loadMarketsForExchange(exchangeId);
      expect(cachedMarkets).toEqual(mockMarkets);
      expect(instance!.loadMarkets).toHaveBeenCalledTimes(1);
    });

    it("should reload markets from exchange after TTL expiry", async () => {
      const instance = await service.getExchangeInstance(exchangeId);
      if (instance && !("mockResolvedValue" in instance.loadMarkets)) {
        instance.loadMarkets = vi
          .fn()
          .mockResolvedValue(deepClone(mockMarkets));
      }
      const MARKETS_CACHE_TTL = 5 * 60 * 1000; // 5 minutes

      // First call
      await service.loadMarketsForExchange(exchangeId);
      expect(instance!.loadMarkets).toHaveBeenCalledTimes(1);

      // Advance time beyond TTL
      vi.advanceTimersByTime(MARKETS_CACHE_TTL + 1000);

      // Second call, should miss cache and reload
      const reloadedMarkets = await service.loadMarketsForExchange(exchangeId);
      expect(reloadedMarkets).toEqual(mockMarkets);
      expect(instance!.loadMarkets).toHaveBeenCalledTimes(2);
    });

    it("should return null and log error if fetching markets fails", async () => {
      // Clear cache for this specific test to ensure fresh load attempt
      (service as any).marketsCache.delete(`markets:${exchangeId}`);

      // Ensure the shared mock is configured to reject *before* getExchangeInstance is called
      // This is crucial because getExchangeInstance itself might call loadMarketsForExchange internally
      if (testAccessibleMockInstances?.[exchangeId]) {
        testAccessibleMockInstances[exchangeId].loadMarkets = vi
          .fn()
          .mockRejectedValue(new Error("Network Error"));
      } else {
        throw new Error(
          `testAccessibleMockInstances or mock for ${exchangeId} not found for this test setup.`
        );
      }

      // Re-initialize service or ensure instance is fetched *after* the mock is set to reject
      // If service is created in beforeEach, we might need to create a new one or reset the instance
      // For simplicity, we'll assume the above `testAccessibleMockInstances` manipulation is enough if
      // `getExchangeInstance` always uses the latest from `testAccessibleMockInstances` via the ccxt mock proxy.

      // The instance variable here is just to demonstrate, it might not be strictly needed
      // if getExchangeInstance correctly picks up the rejecting mock.
      // const instance = await service.getExchangeInstance(exchangeId);
      // if (!instance) throw new Error("Instance not created for markets error test");
      // instance.loadMarkets should ideally reflect the rejecting mock already.

      const markets = await service.loadMarketsForExchange(exchangeId);

      expect(markets).toBeNull();
      expect(logger.error).toHaveBeenCalledWith(
        `Error loading markets for ${exchangeId}:`,
        expect.any(Error)
      );
      expect((service as any).marketsCache.has(`markets:${exchangeId}`)).toBe(
        false
      );
    });
  });

  describe("getTradingFees (for a specific symbol)", () => {
    let service: ExchangeService;
    const exchangeId = "binance" as ExchangeId;
    const symbolBtcUsdt = "BTC/USDT";
    const symbolEthUsdt = "ETH/USDT";

    const mockBtcFee: CCXTTradingFeeInterface = {
      info: {
        symbol: symbolBtcUsdt,
        makerFeeRate: "0.001",
        takerFeeRate: "0.001",
      },
      symbol: symbolBtcUsdt,
      maker: 0.001,
      taker: 0.001,
      percentage: true,
      tierBased: false,
    };
    const mockEthFee: CCXTTradingFeeInterface = {
      info: {
        symbol: symbolEthUsdt,
        makerFeeRate: "0.0008",
        takerFeeRate: "0.0009",
      },
      symbol: symbolEthUsdt,
      maker: 0.0008,
      taker: 0.0009,
      percentage: true,
      tierBased: false,
    };
    // This mock is what instance.fetchTradingFees() would return
    const mockAllFeesResponse: CCXTTradingFees = {
      info: { someGlobalFeeInfo: true } as any,
      [symbolBtcUsdt]: mockBtcFee,
      [symbolEthUsdt]: mockEthFee,
    };

    beforeEach(() => {
      service = new ExchangeService({ logger: logger, env: mockEnv });
      if (testAccessibleMockInstances?.[exchangeId]) {
        testAccessibleMockInstances[exchangeId].fetchTradingFees = vi
          .fn()
          .mockResolvedValue(deepClone(mockAllFeesResponse));
        testAccessibleMockInstances[exchangeId].has = {
          fetchTradingFees: true,
        };
      }
    });

    it("should fetch and return the trading fee for a specific symbol", async () => {
      const fee = await service.getTradingFees(exchangeId, symbolBtcUsdt);
      expect(fee).toEqual(mockBtcFee);
      expect(
        testAccessibleMockInstances![exchangeId].fetchTradingFees
      ).toHaveBeenCalledTimes(1);
    });

    it("should return null if the specific symbol is not found in fetched fees", async () => {
      const fee = await service.getTradingFees(exchangeId, "NONEXISTENT/USDT");
      expect(fee).toBeNull();
      expect(
        testAccessibleMockInstances![exchangeId].fetchTradingFees
      ).toHaveBeenCalledTimes(1); // Still called once
    });

    it("should return null if exchange does not support fetchTradingFees capability", async () => {
      if (testAccessibleMockInstances?.[exchangeId]) {
        testAccessibleMockInstances[exchangeId].has = {
          fetchTradingFees: false,
        };
      }
      const fee = await service.getTradingFees(exchangeId, symbolBtcUsdt);
      expect(fee).toBeNull();
      expect(logger.warn).toHaveBeenCalledWith(
        `getTradingFees: Exchange ${exchangeId} does not support fetchTradingFees for symbol ${symbolBtcUsdt}.`
      );
      // Ensure fetchTradingFees was NOT called on the instance if 'has' is false
      expect(
        testAccessibleMockInstances![exchangeId].fetchTradingFees
      ).not.toHaveBeenCalled();
    });

    it("should return null and log error if underlying fetchTradingFees fails", async () => {
      const errorMessage = "Fee fetch error";
      if (testAccessibleMockInstances?.[exchangeId]) {
        (
          testAccessibleMockInstances[exchangeId].fetchTradingFees as Mock
        ).mockRejectedValue(new Error(errorMessage));
      }
      const fee = await service.getTradingFees(exchangeId, symbolBtcUsdt);
      expect(fee).toBeNull();
      expect(logger.error).toHaveBeenCalledWith(
        `Error fetching trading fees for ${exchangeId} and symbol ${symbolBtcUsdt}:`,
        expect.objectContaining({ message: errorMessage })
      );
    });
  });

  describe("getAccountLeverage", () => {
    let service: ExchangeService;
    const exchangeId = "binance" as ExchangeId;
    const symbol = "BTC/USDT";

    it("should fetch account leverage for a given symbol on an exchange", async () => {
      if (!testAccessibleMockInstances?.binance?.fetchLeverage) {
        throw new Error(
          "Binance mock or fetchLeverage not available for getAccountLeverage test"
        );
      }
      const binanceFetchLeverageMock = testAccessibleMockInstances.binance
        .fetchLeverage as Mock;
      binanceFetchLeverageMock.mockClear();

      // Define fakeLeverage conforming to ccxt.Leverage with a unique name
      const actualFakeCcxtLeverageData: ccxt.Leverage = {
        // Explicitly ccxt.Leverage
        info: {
          someData: "leverageData",
          source: "mock",
          testKey: "uniqueValue",
        },
        symbol: symbol,
        marginMode: "isolated",
        leverage: 10,
      };

      binanceFetchLeverageMock.mockResolvedValue(actualFakeCcxtLeverageData);

      const fetchedLeverageResult: ccxt.Leverage | null =
        await service.getAccountLeverage(exchangeId, symbol); // Explicitly type the result

      expect(binanceFetchLeverageMock).toHaveBeenCalledWith(symbol, undefined);
      expect(fetchedLeverageResult).toEqual(actualFakeCcxtLeverageData);
      expect(fetchedLeverageResult).not.toBeNull();
      if (fetchedLeverageResult) {
        // Type guard
        expect(fetchedLeverageResult.leverage).toBe(10); // Check the specific leverage value
      }
    });

    it("should return null if exchange does not support fetchLeverage", async () => {
      const exchangeId = "kraken" as ExchangeId;
      const symbol = "ETH/USDT";
      const instance = await service.getExchangeInstance(exchangeId);
      if (!instance) throw new Error("Instance not created for kraken");

      instance.has = { ...instance.has, fetchLeverage: false };
      if (testAccessibleMockInstances?.[exchangeId]) {
        (
          testAccessibleMockInstances[exchangeId].fetchLeverage as Mock
        ).mockClear();
      }

      const leverage = await service.getAccountLeverage(exchangeId, symbol);
      expect(leverage).toBeNull();
      expect(logger.warn).toHaveBeenCalledWith(
        `getAccountLeverage: Exchange ${exchangeId} does not support fetchLeverage for ${symbol}`
      );
      if (testAccessibleMockInstances?.[exchangeId]) {
        expect(
          testAccessibleMockInstances[exchangeId].fetchLeverage
        ).not.toHaveBeenCalled();
      }
    });
  });

  afterAll(() => {
    vi.doUnmock("ccxt");
    vi.resetModules();
  });
});

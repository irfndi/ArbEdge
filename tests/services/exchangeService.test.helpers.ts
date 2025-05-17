/// <reference types='@cloudflare/workers-types' />
import { vi, Mock } from "vitest";
import type {
  ExchangeId,
  TradingPairSymbol,
  FundingRateInfo,
  Balances,
  Position,
  Order,
  Market,
  Ticker,
  OHLCV,
  OrderBook,
  Trade,
  CCXTTradingFees,
} from "../../src/types";
import type {
  Currency as CCXTCurrency,
  FundingRateHistory as CCXTFundingRateHistory,
  TransferEntry as CCXTTransferEntry,
} from "ccxt";
import { createMockMarket, MOCK_MARKET_DEFAULTS } from "../mocks/marketMocks";
import { mockPositionFactory } from "../mocks/positionMocks";
import { mockTickerFactory } from "../mocks/tickerMocks";
import { mockOrderFactory } from "../mocks/orderMocks";
import { mockTradeFactory } from "../mocks/tradeMocks";
import { MOCK_TRADING_FEES_FACTORY } from "../mocks/feeMocks";
import { MOCK_TIMESTAMP } from "../mocks/mockUtils";
import * as ccxt from "ccxt";

// This interface was in exchangeService.test.ts, moved here as createMockInstance returns it.
export interface MockExchangeInstance {
  // ccxt.Exchange
  id: string;
  loadMarkets: Mock<() => Promise<Record<string, Market>>>;
  fetchMarkets: Mock<() => Promise<Market[]>>;
  fetchTicker: Mock<(symbol: string) => Promise<Ticker>>;
  fetchTickers: Mock<(symbols?: string[]) => Promise<Record<string, Ticker>>>;
  fetchOrderBook: Mock<(symbol: string, limit?: number) => Promise<OrderBook>>;
  fetchOHLCV: Mock<
    (
      symbol: string,
      timeframe?: string,
      since?: number,
      limit?: number
    ) => Promise<OHLCV[]>
  >;
  fetchTrades: Mock<
    (symbol: string, since?: number, limit?: number) => Promise<Trade[]>
  >;
  fetchBalance: Mock<() => Promise<Balances>>;
  createOrder: Mock<
    (
      symbol: string,
      type: string,
      side: string,
      amount: number,
      price?: number,
      params?: Record<string, unknown>
    ) => Promise<Order>
  >;
  cancelOrder: Mock<
    (
      id: string,
      symbol?: string,
      params?: Record<string, unknown>
    ) => Promise<Order>
  >;
  fetchOrder: Mock<
    (
      id: string,
      symbol?: string,
      params?: Record<string, unknown>
    ) => Promise<Order>
  >;
  fetchOpenOrders: Mock<
    (
      symbol?: string,
      since?: number,
      limit?: number,
      params?: Record<string, unknown>
    ) => Promise<Order[]>
  >;
  fetchClosedOrders: Mock<
    (
      symbol?: string,
      since?: number,
      limit?: number,
      params?: Record<string, unknown>
    ) => Promise<Order[]>
  >;
  fetchMyTrades: Mock<
    (
      symbol?: string,
      since?: number,
      limit?: number,
      params?: Record<string, unknown>
    ) => Promise<Trade[]>
  >;
  fetchPositions: Mock<
    (
      symbols?: string[],
      params?: Record<string, unknown>
    ) => Promise<Position[]>
  >;
  fetchFundingRate: Mock<
    (
      symbol: string,
      params?: Record<string, unknown>
    ) => Promise<FundingRateInfo>
  >;
  fetchFundingRates: Mock<
    (
      symbols?: string[],
      params?: Record<string, unknown>
    ) => Promise<Record<string, FundingRateInfo>>
  >;
  fetchFundingRateHistory: Mock<
    (
      symbol?: string,
      since?: number,
      limit?: number,
      params?: Record<string, unknown>
    ) => Promise<CCXTFundingRateHistory[]>
  >;
  setLeverage: Mock<
    (
      leverage: number,
      symbol?: string,
      params?: Record<string, unknown>
    ) => Promise<unknown>
  >;
  transfer: Mock<
    (
      code: string,
      amount: number,
      fromAccount: string,
      toAccount: string,
      params?: Record<string, unknown>
    ) => Promise<CCXTTransferEntry>
  >;
  fetchTradingFees: Mock<
    (params?: Record<string, unknown>) => Promise<CCXTTradingFees>
  >;
  fetchLeverage: Mock<
    (symbol: string, params?: Record<string, unknown>) => Promise<ccxt.Leverage>
  >;
  has: Record<string, boolean | string>;
  markets: Record<string, Market>;
  currencies: Record<string, CCXTCurrency>;
  verbose?: boolean;
  options?: Record<string, unknown>;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [key: string]: unknown;
}

export function MOCK_BINANCE_BALANCES_FACTORY(): Balances {
  return {
    // biome-ignore lint/suspicious/noExplicitAny: CCXTBalances.info is type 'any'
    info: {} as any,
    USDT: { free: 1000, used: 0, total: 1000 },
    BTC: { free: 1, used: 0, total: 1 },
    ETH: { free: 10, used: 0, total: 10 },
  };
}

export function MOCK_FUNDING_RATE_INFO_FACTORY(): FundingRateInfo {
  return {
    symbol: {
      symbol: "BTC/USDT:USDT",
      base: "BTC",
      quote: "USDT",
      type: "swap",
    },
    exchange: "binance" as ExchangeId,
    fundingRate: 0.0001,
    timestamp: MOCK_TIMESTAMP,
    datetime: new Date(MOCK_TIMESTAMP).toISOString(),
    markPrice: 20000,
    indexPrice: 19990,
  };
}

export function MOCK_CCXT_FUNDING_RATE_HISTORY_ITEM_FACTORY(): CCXTFundingRateHistory {
  return {
    symbol: "BTC/USDT:USDT",
    timestamp: MOCK_TIMESTAMP,
    datetime: new Date(MOCK_TIMESTAMP).toISOString(),
    fundingRate: 0.0001,
    info: {},
  };
}

export function MOCK_CCXT_TRANSFER_ENTRY_FACTORY(): CCXTTransferEntry {
  return {
    id: "mocktransfer123",
    timestamp: MOCK_TIMESTAMP,
    datetime: new Date(MOCK_TIMESTAMP).toISOString(),
    currency: "USDT",
    amount: 100,
    fromAccount: "spot",
    toAccount: "futures",
    status: "ok",
    info: {},
  };
}

export function createMockInstance(
  exchangeId: ExchangeId
): MockExchangeInstance {
  const pair: TradingPairSymbol = "BTC/USDT";
  const base: string = "BTC";
  const quote: string = "USDT";
  const marketDefaultsForPair: Market = {
    ...MOCK_MARKET_DEFAULTS,
    symbol: pair,
    base,
    quote,
  } as Market;
  const mockMarketForInstance: Market = createMockMarket({
    created: MOCK_TIMESTAMP,
    symbol: pair,
    base,
    quote,
  });
  const mockBalance = MOCK_BINANCE_BALANCES_FACTORY();
  const mockPositions = [
    mockPositionFactory({
      symbol: "BTC/USDT" as TradingPairSymbol,
      side: "long",
    }),
  ];
  const symbolForFundingRatesKey = mockMarketForInstance?.symbol ?? "BTC/USDT";

  const MOCK_CCXT_LEVERAGE_DATA: ccxt.Leverage = {
    info: { mock: true },
    symbol: "BTC/USDT",
    marginMode: "isolated",
    leverage: 10,
  };

  const mockFetchLeverage = vi.fn<
    [string, Record<string, unknown>?],
    Promise<ccxt.Leverage>
  >();

  return {
    id: exchangeId,
    loadMarkets: vi
      .fn()
      .mockResolvedValue({ "BTC/USDT": mockMarketForInstance }),
    fetchMarkets: vi.fn().mockResolvedValue([mockMarketForInstance]),
    fetchTicker: vi.fn().mockResolvedValue(mockTickerFactory("BTC/USDT")),
    fetchTickers: vi
      .fn()
      .mockResolvedValue({ "BTC/USDT": mockTickerFactory("BTC/USDT") }),
    fetchOHLCV: vi
      .fn()
      .mockResolvedValue([[Date.now(), 1, 2, 0, 1, 100]] as OHLCV[]),
    fetchOrderBook: vi.fn().mockResolvedValue({
      symbol: "BTC/USDT",
      bids: [],
      asks: [],
      timestamp: MOCK_TIMESTAMP,
      datetime: new Date(MOCK_TIMESTAMP).toISOString(),
      nonce: undefined,
    }),
    createOrder: vi.fn().mockResolvedValue(mockOrderFactory()),
    cancelOrder: vi.fn().mockResolvedValue(undefined),
    fetchOrder: vi.fn().mockResolvedValue(mockOrderFactory()),
    fetchOpenOrders: vi.fn().mockResolvedValue([mockOrderFactory()]),
    fetchClosedOrders: vi
      .fn()
      .mockResolvedValue([mockOrderFactory({ status: "closed" })]),
    fetchMyTrades: vi.fn().mockResolvedValue([mockTradeFactory()]),
    fetchTrades: vi.fn().mockResolvedValue([mockTradeFactory()]),
    fetchBalance: vi.fn().mockResolvedValue(mockBalance),
    fetchPositions: vi.fn().mockResolvedValue(mockPositions),
    fetchFundingRate: vi
      .fn()
      .mockResolvedValue(MOCK_FUNDING_RATE_INFO_FACTORY()),
    fetchFundingRates: vi.fn().mockResolvedValue({
      [symbolForFundingRatesKey]: MOCK_FUNDING_RATE_INFO_FACTORY(),
    }),
    fetchFundingRateHistory: vi
      .fn()
      .mockResolvedValue([MOCK_CCXT_FUNDING_RATE_HISTORY_ITEM_FACTORY()]),
    setLeverage: vi.fn().mockResolvedValue({ info: "Leverage set" }),
    transfer: vi.fn().mockResolvedValue(MOCK_CCXT_TRANSFER_ENTRY_FACTORY()),
    fetchTradingFees: vi.fn().mockResolvedValue(MOCK_TRADING_FEES_FACTORY()),
    fetchLeverage: mockFetchLeverage,
    has: {
      fetchMarkets: true,
      fetchOHLCV: true,
      fetchTicker: true,
      fetchTickers: true,
      fetchOrderBook: true,
      createOrder: true,
      cancelOrder: true,
      fetchBalance: true,
      fetchMyTrades: true,
      fetchPositions: true,
      fetchFundingRate: true,
      setLeverage: true,
      fetchTrades: true,
      fetchOrder: true,
      fetchOpenOrders: true,
      fetchClosedOrders: true,
      fetchFundingRates: true,
      fetchFundingRateHistory: true,
      transfer: true,
      fetchTradingFees: true,
      fetchLeverage: true,
    } as Record<string, boolean | undefined | Mock>,
    markets: { [pair]: marketDefaultsForPair } as Record<
      TradingPairSymbol,
      Market
    >,
    currencies: {
      [base]: {
        id: base,
        code: base,
        precision: 8,
        name: base,
      } as CCXTCurrency,
      [quote]: {
        id: quote,
        code: quote,
        precision: 8,
        name: quote,
      } as CCXTCurrency,
    },
    options: {},
    verbose: false,
  } as MockExchangeInstance;
}

export const ALL_MOCK_EXCHANGE_IDS: ExchangeId[] = [
  "binance",
  "bybit",
  "bitget",
  "kraken",
  "mexc",
  "okx",
  "bingx",
];

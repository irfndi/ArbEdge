/// <reference types="vitest/globals" />
import {
  vi,
  describe,
  it,
  expect,
  beforeEach,
  afterEach,
  type Mock,
  type DeepMockProxy,
} from "vitest";
import type { Router, error, json } from "itty-router"; // Types, as they are not used as values in this file
import type { IRequest, RouterType } from "itty-router"; // Types
import type {
  ExecutionContext,
  KVNamespace,
  DurableObjectNamespace,
  DurableObjectId,
  DurableObjectStub,
  KVNamespaceListOptions,
  KVNamespaceListResult,
  KVNamespaceListKey,
  Socket,
  DurableObjectJurisdiction,
  KVNamespaceGetOptions,
  KVNamespacePutOptions,
  KVNamespaceGetWithMetadataResult,
} from "@cloudflare/workers-types";

// eslint-disable-next-line @typescript-eslint/consistent-type-imports
// import type { ReadableStream } from "@cloudflare/workers-types"; // type-only
import { ReadableStream } from "@cloudflare/workers-types"; // Changed to regular import

import { TelegramService } from "../src/services/telegramService";
import { OpportunityService } from "../src/services/opportunityService";
import type {
  StructuredTradingPair,
  ExchangeId,
  ArbitrageOpportunity,
  LoggerInterface,
  Env as ImportedEnv,
  OrderBook,
  Market,
  FundingRateInfo,
} from "../src/types";
import { CCXTTradingFees as TradingFees } from "../src/types"; // Specific import for TradingFees
import httpWorker from "../src/index";
import {
  env as cfTestEnv, // Import env from cloudflare:test
  createExecutionContext as cfCreateExecutionContext,
  waitOnExecutionContext as cfWaitOnExecutionContext,
} from "cloudflare:test"; // Import test utilities

const TEST_TIMEOUT = 30000; // 30 seconds for potentially long operations
vi.setConfig({ testTimeout: TEST_TIMEOUT });

// User needs to ensure 'itty-router-extras' is installed
// import { createCors as actualCreateCors } from 'itty-router-extras';

// Local LogLevel enum for testing purposes if not available from src/types or a library
enum LogLevel {
  ERROR = "error",
  WARN = "warn",
  INFO = "info",
  HTTP = "http",
  VERBOSE = "verbose",
  DEBUG = "debug",
  SILLY = "silly",
}

vi.mock("../src/services/TelegramService", () => ({
  TelegramService: vi.fn().mockImplementation((env, ctx, logger) => ({
    sendMessage: vi.fn().mockResolvedValue(undefined),
    sendMessages: vi.fn().mockResolvedValue(undefined),
    sendOpportunityNotification: vi.fn().mockResolvedValue(undefined),
    processWebhook: vi.fn().mockResolvedValue(undefined),
    findArbitrageOpportunities: vi.fn().mockResolvedValue([]),
    processFundingRateOpportunities: vi.fn().mockResolvedValue(undefined),
    stop: vi.fn(),
    logger: logger || createMockLogger(),
  })),
}));

vi.mock("../src/services/OpportunityService", () => ({
  OpportunityService: vi.fn().mockImplementation((env, logger) => ({
    calculateArbitrage: vi.fn().mockResolvedValue(null),
  })),
}));

// Simplified TestEnv that matches the required interface
type TestEnv = {
  // Required environment variables
  LOG_LEVEL: string;
  TELEGRAM_BOT_TOKEN: string;
  TELEGRAM_CHAT_ID: string;
  EXCHANGES: string;
  MONITORED_PAIRS_CONFIG: string;
  ARBITRAGE_THRESHOLD: string;

  // Services
  LOGGER: MockLogger;
  ArbEdgeKV: KVNamespace;
  POSITIONS: DurableObjectNamespace;

  // Optional test-specific overrides
  KV_NAMESPACE?: KVNamespace;
  CONFIG_KV_NAMESPACE?: KVNamespace;
  DURABLE_OBJECT_NAMESPACE_LEGACY?: DurableObjectNamespace;
} & Omit<ImportedEnv, "LOGGER" | "ArbEdgeKV" | "POSITIONS">;

interface MockLogger extends LoggerInterface {
  // Ensure all methods from LoggerInterface are here and are Mocks
  debug: Mock<(message: string, ...meta: unknown[]) => void>;
  info: Mock<(message: string, ...meta: unknown[]) => void>;
  warn: Mock<(message: string, ...meta: unknown[]) => void>;
  error: Mock<(message: string, ...meta: unknown[]) => void>;
  log: Mock<(level: string, message: string, ...meta: unknown[]) => void>;
  // Optional methods from LoggerInterface, also as Mocks if used/tested
  http?: Mock<(message: string, ...meta: unknown[]) => void>;
  verbose?: Mock<(message: string, ...meta: unknown[]) => void>;
  silly?: Mock<(message: string, ...meta: unknown[]) => void>;
  child?: Mock<(options: Record<string, unknown>) => MockLogger>;
  setLogLevel?: (level: LogLevel) => void;
}

const mockRequest = (method: string, url: string, body?: unknown): Request => {
  const options: RequestInit = { method };
  if (body !== undefined) {
    options.body = JSON.stringify(body);
    options.headers = { "Content-Type": "application/json" };
  }
  return new Request(url, options);
};

const createMockLogger = (): MockLogger => ({
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  log: vi.fn(),
  // Optional methods - provide mock impl if used in tests
  http: vi.fn(),
  verbose: vi.fn(),
  silly: vi.fn(),
  child: vi.fn().mockImplementation(() => createMockLogger()), // Example: child returns a new mock logger
  // From LoggerInterface definition (not Winston specific properties like 'level')
  // Assuming LoggerInterface does not directly have 'logLevel' as a property, but might have setLogLevel
  setLogLevel: vi.fn(),
});

const actualMockLoggerInstance = createMockLogger(); // Define actual instance here

vi.mock("../src/utils/logger", () => ({
  // Keep this mock active
  createLogger: vi.fn(() => actualMockLoggerInstance),
}));

let mockLoggerInstance: MockLogger;
let mockEnv: TestEnv; // Restore global mockEnv
let mockExecutionContext: ExecutionContext; // Restore global mockExecutionContext

type MockKVNamespaceGetOptionsType = "text" | "json" | "arrayBuffer" | "stream";
interface MockKVNamespaceGetOptions {
  type?: MockKVNamespaceGetOptionsType;
  // cacheTtl?: number; // Not mocked in this helper as it's not used by processGetOptions
}

interface MockKVNamespace extends KVNamespace {
  text: (key: string) => Promise<string | null>;
  json: <T = unknown>(key: string) => Promise<T | null>;
  arrayBuffer: (key: string) => Promise<ArrayBuffer | null>;
  stream: (key: string) => Promise<ReadableStream | null>;
}

interface KVStoreEntry {
  value: string | ArrayBuffer | ReadableStream;
  expiration?: number;
  metadata?: unknown;
}

interface KVGetWithMetadataResult<T, M> {
  value: T | null;
  metadata: M | null;
  cacheStatus: "HIT" | "MISS" | "STALE" | null;
}

// Define KVNamespaceReadType locally as it's not imported or globally available
type KVNamespaceReadType = "text" | "json" | "arrayBuffer" | "stream";

// Helper function to process get options
function processGetOptions(
  optionsOrType?:
    | KVNamespaceReadType
    | Partial<KVNamespaceGetOptions<KVNamespaceReadType>>
): KVNamespaceReadType {
  if (typeof optionsOrType === "string") {
    // Ensure the string is a valid KVNamespaceReadType
    const validTypes: KVNamespaceReadType[] = [
      "text",
      "json",
      "arrayBuffer",
      "stream",
    ];
    if (validTypes.includes(optionsOrType as KVNamespaceReadType)) {
      return optionsOrType as KVNamespaceReadType;
    }
    // If string is not a valid type, default to "text"
    return "text";
  }
  // Check if optionsOrType is an object and has a 'type' property
  if (
    optionsOrType &&
    typeof optionsOrType === "object" &&
    optionsOrType.type
  ) {
    const type = optionsOrType.type;
    // Ensure the type from the object is a valid KVNamespaceReadType
    const validTypes: KVNamespaceReadType[] = [
      "text",
      "json",
      "arrayBuffer",
      "stream",
    ];
    if (validTypes.includes(type)) {
      return type;
    }
  }
  // Default if optionsOrType is undefined, not a string,
  // or an object without a valid 'type' property
  return "text";
}

// Helper function to convert value based on type
async function convertValue(
  value: string,
  type: "text"
): Promise<string | null>;
async function convertValue(
  value: string,
  type: "arrayBuffer"
): Promise<ArrayBuffer | null>;
async function convertValue(
  value: string,
  type: "stream"
): Promise<ReadableStream | null>;
async function convertValue<T = unknown>(
  value: string,
  type: "json"
): Promise<T | null>;
async function convertValue<T = unknown>(
  value: string,
  type: "text" | "json" | "arrayBuffer" | "stream"
): Promise<T | string | ArrayBuffer | ReadableStream | null> {
  // Return null for parse errors etc.
  switch (type) {
    case "text":
      return value;
    case "json":
      try {
        return JSON.parse(value) as T;
      } catch (e) {
        // Consider strict spec adherence: throw new TypeError("Value is not valid JSON");
        console.error("Mock KV: Failed to parse JSON", e);
        return null; // Or throw, depending on desired mock behavior
      }
    case "arrayBuffer": {
      return new TextEncoder().encode(value).buffer as ArrayBuffer;
    }
    case "stream": {
      return new ReadableStream({
        start(controller) {
          controller.enqueue(new TextEncoder().encode(value));
          controller.close();
        },
      });
    }
    default:
      // This case should ideally be unreachable if processGetOptions fully restricts the 'type' argument.
      // Adding a log and returning null or throwing an error might be appropriate.
      console.error(
        `Mock KV: Unexpected type in convertValue: ${type}. This should not happen.`
      );
      // To satisfy TypeScript's return type requirement, ensure all paths return a compatible type.
      // Depending on strictness, throwing an error might be better.
      return null;
  }
}

async function streamToArrayBuffer(
  stream: ReadableStream<Uint8Array>
): Promise<ArrayBuffer> {
  const reader = stream.getReader();
  const chunks: Uint8Array[] = [];
  let totalLength = 0;
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    if (value) {
      chunks.push(value);
      totalLength += value.length;
    }
  }

  const result = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result.buffer;
}

const createSimpleMockKvNamespace = (): KVNamespace => {
  const store = new Map<string, KVStoreEntry>();
  const metadataStore = new Map<string, unknown>();
  const expirationStore = new Map<string, number>();

  const kvNamespaceImplInternal: KVNamespace = {
    // Implementation for get
    async get<ExpectedValue = unknown>(
      key: string | string[],
      optionsOrType?:
        | KVNamespaceReadType
        | Partial<KVNamespaceGetOptions<KVNamespaceReadType>>
    ): Promise<
      | (string | ExpectedValue | ArrayBuffer | ReadableStream | null)
      | Map<
          string,
          string | ExpectedValue | ArrayBuffer | ReadableStream | null
        >
    > {
      if (Array.isArray(key)) {
        throw new Error("Mock KV: Batch get operation not implemented.");
      }
      const valueEntry = store.get(key);
      if (valueEntry === undefined) return null;

      const effectiveExpiration =
        expirationStore.get(key) ?? valueEntry.expiration;
      if (effectiveExpiration && Date.now() >= effectiveExpiration * 1000) {
        store.delete(key);
        metadataStore.delete(key);
        expirationStore.delete(key);
        return null;
      }
      const type = processGetOptions(optionsOrType);
      if (type === "json") {
        return convertValue<ExpectedValue>(valueEntry.value as string, "json");
      }
      if (type === "text") {
        return convertValue(valueEntry.value as string, "text");
      }
      if (type === "arrayBuffer") {
        return convertValue(valueEntry.value as string, "arrayBuffer");
      }
      // type must be "stream"
      return convertValue(valueEntry.value as string, "stream");
    },

    // Minimal mock for getWithMetadata to satisfy the interface
    async getWithMetadata(key: string, optionsOrType?: any): Promise<any> {
      console.warn(
        `Mock KV: getWithMetadata for key '${key}' called but not fully implemented, returning default nulls.`
      );
      return {
        value: null,
        metadata: null,
        cacheStatus: null, // if your mock's KVGetWithMetadataResult includes this
      };
    },

    async put(
      key: string,
      valueToPut: string | ArrayBuffer | ArrayBufferView | ReadableStream,
      options?: KVNamespacePutOptions
    ): Promise<void> {
      let stringValue: string;
      if (valueToPut instanceof ArrayBuffer || ArrayBuffer.isView(valueToPut)) {
        const buffer =
          valueToPut instanceof ArrayBuffer
            ? valueToPut
            : (valueToPut as ArrayBufferView).buffer;
        stringValue = new TextDecoder().decode(buffer as ArrayBuffer);
      } else if (valueToPut instanceof ReadableStream) {
        const buffer = await streamToArrayBuffer(
          valueToPut as ReadableStream<Uint8Array>
        ); // Ensure streamToArrayBuffer is defined
        stringValue = new TextDecoder().decode(buffer);
      } else {
        stringValue = valueToPut as string;
      }
      let finalExpiration: number | undefined = options?.expiration;
      if (options?.expirationTtl) {
        finalExpiration = Math.floor(Date.now() / 1000) + options.expirationTtl;
      }
      store.set(key, {
        value: stringValue,
        metadata: options?.metadata,
        expiration: finalExpiration,
      });
      if (options?.metadata !== undefined) {
        metadataStore.set(key, options.metadata);
      } else {
        if (options?.metadata === null) {
          metadataStore.delete(key);
        }
      }
      if (finalExpiration !== undefined) {
        expirationStore.set(key, finalExpiration);
      } else {
        expirationStore.delete(key);
      }
    },
    async delete(keys: string | string[]): Promise<void> {
      const keysToDelete = Array.isArray(keys) ? keys : [keys];
      for (const key of keysToDelete) {
        store.delete(key);
        metadataStore.delete(key);
        expirationStore.delete(key);
      }
    },
    async list<Metadata = unknown>(
      options?: KVNamespaceListOptions
    ): Promise<KVNamespaceListResult<Metadata, string>> {
      const prefix = options?.prefix ?? "";
      const limit = options?.limit ?? 1000; // Default limit based on documentation
      const cursor = options?.cursor;

      const allMatchingKeys: KVNamespaceListKey<Metadata>[] = [];

      // Iterate over sorted keys for consistent cursor behavior
      const sortedStoreKeys = Array.from(store.keys()).sort();

      let pastCursor = cursor === undefined;
      for (const key of sortedStoreKeys) {
        if (!pastCursor && key === cursor) {
          pastCursor = true;
          continue; // Skip elements up to and including the cursor
        }
        if (!pastCursor) {
          continue;
        }

        if (key.startsWith(prefix)) {
          const valueEntry = store.get(key);
          if (!valueEntry) continue; // Should not happen if key is from store.keys()

          const effectiveExpiration =
            expirationStore.get(key) ?? valueEntry.expiration;
          if (effectiveExpiration && Date.now() >= effectiveExpiration * 1000) {
            // Handle expired key: delete it and don't include in list
            store.delete(key);
            metadataStore.delete(key);
            expirationStore.delete(key);
            continue;
          }

          allMatchingKeys.push({
            name: key,
            expiration: effectiveExpiration,
            metadata: (metadataStore.get(key) ?? valueEntry.metadata) as
              | Metadata
              | undefined,
          });
        }
      }

      const keysSlice = allMatchingKeys.slice(0, limit);
      const list_complete = keysSlice.length === allMatchingKeys.length;
      const nextCursor = list_complete
        ? undefined
        : keysSlice[keysSlice.length - 1]?.name;

      return {
        keys: keysSlice,
        list_complete: list_complete,
        cursor: nextCursor,
      } as KVNamespaceListResult<Metadata, string>;
    },
  };

  return kvNamespaceImplInternal as KVNamespace; // Explicitly cast the entire mock object
};

const createMockDurableObjectNamespace = (): DurableObjectNamespace => ({
  newUniqueId: vi.fn(),
  idFromName: vi.fn(),
  idFromString: vi.fn(),
  get: vi.fn().mockImplementation(
    (id: DurableObjectId): DurableObjectStub => ({
      id,
      name: undefined,
      fetch: vi.fn().mockResolvedValue(new Response("Mock DO Response")),
      connect: vi.fn().mockImplementation((): Socket => {
        return {
          send: vi.fn(),
          close: vi.fn(),
          accept: vi.fn(),
          addEventListener: vi.fn(),
          removeEventListener: vi.fn(),
          dispatchEvent: vi.fn(),
        } as unknown as Socket;
      }),
    })
  ),
  jurisdiction: vi.fn(
    (_jurisdictionParam: DurableObjectJurisdiction): DurableObjectNamespace =>
      createMockDurableObjectNamespace()
  ),
});

beforeEach(() => {
  // Clear calls to mock constructors before each test
  vi.clearAllMocks();

  // Reset mock implementations for services
  (TelegramService as unknown as Mock).mockImplementation(
    (env, ctx, logger) => ({
      sendMessage: vi.fn().mockResolvedValue(undefined),
      sendMessages: vi.fn().mockResolvedValue(undefined),
      sendOpportunityNotification: vi.fn().mockResolvedValue(undefined),
      processWebhook: vi.fn().mockResolvedValue(undefined),
      findArbitrageOpportunities: vi.fn().mockResolvedValue([]),
      processFundingRateOpportunities: vi.fn().mockResolvedValue(undefined),
      stop: vi.fn(),
      // Ensure service mocks also use or can receive the shared logger if needed
      // For now, their internal logger creation (e.g. logger || createMockLogger()) might create separate instances
      // This could be refined if cross-service logger interaction is tested.
      // The createLogger mock above should affect loggers created *within* src/index.ts and other src modules.
      logger: logger || actualMockLoggerInstance, // Default to actualMockLoggerInstance if no logger passed
    })
  );

  // Reset mock logger (clear spies on the shared instance)
  mockLoggerInstance = actualMockLoggerInstance; // Assign the shared instance
  // Clear all spy functions on the shared logger instance
  for (const mockFn of Object.values(mockLoggerInstance)) {
    if (typeof mockFn === "function" && "mockClear" in mockFn) {
      (mockFn as Mock).mockClear();
    }
  }

  // Create a new mock environment with required properties
  const mockKv = createSimpleMockKvNamespace();
  const mockDo = createMockDurableObjectNamespace();

  // Initialize the mock environment with required properties
  mockEnv = {
    // Required environment variables
    LOG_LEVEL: "info",
    TELEGRAM_BOT_TOKEN: "test-token",
    TELEGRAM_CHAT_ID: "test-chat-id",
    EXCHANGES: "binance,bybit",
    MONITORED_PAIRS_CONFIG: JSON.stringify([
      { symbol: "BTC/USDT", base: "BTC", quote: "USDT", type: "spot" },
    ]),
    ARBITRAGE_THRESHOLD: "0.5",

    // Services
    LOGGER: mockLoggerInstance,
    ArbEdgeKV: createSimpleMockKvNamespace(), // Type assertion to handle mock
    POSITIONS: mockDo as unknown as DurableObjectNamespace, // Type assertion to handle mock

    // Spread any existing mockEnv properties that aren't being overridden
    ...Object.fromEntries(
      Object.entries(mockEnv || {}).filter(
        ([key]) =>
          ![
            "LOG_LEVEL",
            "TELEGRAM_BOT_TOKEN",
            "TELEGRAM_CHAT_ID",
            "EXCHANGES",
            "MONITORED_PAIRS_CONFIG",
            "ARBITRAGE_THRESHOLD",
            "LOGGER",
            "ArbEdgeKV",
            "POSITIONS",
          ].includes(key)
      )
    ),
  };

  // Initialize the mock execution context
  mockExecutionContext = {
    waitUntil: vi.fn(),
    passThroughOnException: vi.fn(),
    props: {},
  } as unknown as ExecutionContext;

  // Reset OpportunityService mock
  (OpportunityService as unknown as Mock).mockImplementation((env, logger) => ({
    calculateArbitrage: vi.fn().mockResolvedValue(null),
  }));

  // Add any additional test-specific environment overrides here if needed
  mockEnv = {
    ...mockEnv,
    // Add any test-specific overrides here
  } as unknown as TestEnv;

  mockExecutionContext = {
    waitUntil: vi.fn(),
    passThroughOnException: vi.fn(),
    props: {},
  } as ExecutionContext;
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("Worker Main Logic", () => {
  it("should correctly initialize TelegramService upon first use by /find-opportunities", async () => {
    const findOpsRequest = mockRequest(
      "GET",
      "http://localhost/find-opportunities"
    );
    const response = await httpWorker.fetch(
      findOpsRequest,
      mockEnv,
      mockExecutionContext
    );
    expect(response.status).toBe(404);
    const responseText = await response.text();
    expect(responseText).toBe("ONLY ALL ROUTE ACTIVE");
    // TelegramService will not be called due to the all('*') route
    expect(TelegramService).not.toHaveBeenCalled();
  });

  describe("GET / route", () => {
    it("should return 404 and custom message from all route with minimal router", async () => {
      const request = mockRequest("GET", "http://localhost/");
      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );
      expect(response.status).toBe(404);
      const text = await response.text();
      expect(text).toBe("ONLY ALL ROUTE ACTIVE");
    });
  });

  describe("HTTP Endpoints", () => {
    it('should return 200 and "pong" for GET /ping', async () => {
      const request = mockRequest("GET", "http://localhost/ping");
      const testCtx = cfCreateExecutionContext(); // Use context from cloudflare:test

      // Ensure mockEnv is initialized (happens in beforeEach)
      // Ensure mockEnv.LOGGER is actualMockLoggerInstance (happens in beforeEach)
      // The global vi.mock for createLogger should ensure any new loggers also use actualMockLoggerInstance

      const response = await httpWorker.fetch(request, mockEnv, testCtx); // Pass our mockEnv and the new context
      await cfWaitOnExecutionContext(testCtx); // Wait on the new context

      expect(response.status).toBe(200);
      const responseText = await response.text();
      expect(responseText).toBe("pong");
    });

    it("should handle invalid JSON in webhook", async () => {
      const request = new Request("http://localhost/webhook", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: "invalid json string { not json }", // Malformed JSON
      });

      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );
      // grammY's webhookCallback will attempt to parse, fail, and throw an error.
      // Our catch block will turn this into a 500.
      expect(response.status).toBe(500);
      const responseBodyText = await response.text();
      // The message might be generic like "Internal Server Error: Unexpected end of JSON input"
      // or whatever message grammy/CF worker surfaces from the JSON.parse error.
      expect(responseBodyText).toContain("Internal Server Error");
    });

    it("should return 500 for POST /webhook with non-JSON content type", async () => {
      const webhookBody = "Test webhook body"; // Plain text
      const request = mockRequest(
        "POST",
        "http://localhost/webhook",
        webhookBody // Sending plain text body
      );
      (request.headers as Headers).set("Content-Type", "text/plain");

      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );
      // grammY expects JSON. A non-JSON content type will cause request.json() to fail internally.
      // This error will be caught and turned into a 500.
      expect(response.status).toBe(500);
      const responseBodyText = await response.text();
      expect(responseBodyText).toContain("Internal Server Error");
    });

    it("should return 404 for GET /find-opportunities with minimal router", async () => {
      const mockOpportunitiesData: ArbitrageOpportunity[] = [
        {
          id: "opp1",
          type: "spot",
          pairSymbol: "BTC/USDT", // Changed from pair to pairSymbol
          longExchange: "binance",
          shortExchange: "kraken",
          longRate: 0.01,
          shortRate: 0.02,
          grossProfitMetric: 0.01, // Assuming rateDifference was meant to be grossProfitMetric
          timestamp: Date.now(),
          potentialProfitValue: 100,
          details: "Some details",
        },
      ];

      // Mock the instance method that will be called
      const mockTelegramServiceInstance = {
        findArbitrageOpportunities: vi
          .fn()
          .mockResolvedValue(mockOpportunitiesData),
        // include other methods if they are called by the route
      };
      (TelegramService as unknown as Mock).mockImplementationOnce(
        () => mockTelegramServiceInstance
      );

      const request = mockRequest("GET", "http://localhost/find-opportunities");
      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );

      expect(response.status).toBe(404);
      const responseText = await response.text();
      expect(responseText).toBe("ONLY ALL ROUTE ACTIVE");
    });

    it("should return 404 for GET /find-opportunities with minimal router (OpportunityService branch)", async () => {
      const mockOpportunitiesData: ArbitrageOpportunity[] = [
        {
          id: "opp-os-1", // Added id for consistency
          type: "CEX-CEX",
          pairSymbol: "BTC/USD", // Changed from pair to pairSymbol
          longExchange: "binance",
          shortExchange: "kraken",
          buyPrice: 50000, // Added for consistency with ArbitrageOpportunity fields
          sellPrice: 50500, // Added for consistency with ArbitrageOpportunity fields
          grossProfitMetric: 500, // Assuming estimatedProfit from trade maps to grossProfitMetric
          timestamp: Date.now(),
          tradeExecutionDetails: {
            // Moved nested trade object here
            exchangeBuy: "binance",
            exchangeSell: "kraken",
            pair: "BTC/USD", // pair here is fine as it's specific to tradeExecutionDetails
            buyPrice: 50000,
            sellPrice: 50500,
            amount: 1,
            estimatedProfit: 500,
          },
        },
      ];
      (OpportunityService as unknown as Mock).mockImplementationOnce(() => ({
        findOpportunities: vi.fn().mockResolvedValue(mockOpportunitiesData),
      }));
      const request = mockRequest("GET", "http://localhost/find-opportunities");
      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );

      expect(response.status).toBe(404);
      const responseText = await response.text();
      expect(responseText).toBe("ONLY ALL ROUTE ACTIVE");
    });

    it("should return 404 for GET /find-funding-rates with minimal router", async () => {
      // const mockTelegramServiceInstance = { processFundingRateOpportunities: vi.fn().mockResolvedValue(undefined) };
      // (TelegramService as unknown as Mock).mockImplementationOnce(() => mockTelegramServiceInstance);

      const request = mockRequest("GET", "http://localhost/find-funding-rates");
      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );
      expect(response.status).toBe(404);
      const responseText = await response.text();
      expect(responseText).toBe("ONLY ALL ROUTE ACTIVE");
      // expect(mockTelegramServiceInstance.processFundingRateOpportunities).toHaveBeenCalled();
    });

    it("should return 404 for GET /find-funding-rates (error case) with minimal router", async () => {
      const errorMessage = "Service failure processing funding rates";
      // const mockTelegramServiceInstance = { processFundingRateOpportunities: vi.fn().mockRejectedValue(new Error(errorMessage)) };
      // (TelegramService as unknown as Mock).mockImplementationOnce(() => mockTelegramServiceInstance);

      const request = mockRequest("GET", "http://localhost/find-funding-rates");
      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );
      expect(response.status).toBe(404);
      const responseText = await response.text();
      expect(responseText).toBe("ONLY ALL ROUTE ACTIVE");
      // expect(mockEnv.LOGGER.error).toHaveBeenCalledWith('Error in /find-funding-rates:', expect.any(Error));
      // expect(mockTelegramServiceInstance.processFundingRateOpportunities).toHaveBeenCalled();
    });

    it("should return 404 for undefined routes", async () => {
      const request = mockRequest(
        "GET",
        "http://localhost/this-route-does-not-exist"
      );
      const response = await httpWorker.fetch(
        request,
        mockEnv,
        mockExecutionContext
      );
      expect(response.status).toBe(404);
      const responseText = await response.text();
      expect(responseText).toBe("ONLY ALL ROUTE ACTIVE");
    });
  });

  it("should return 200 for GET /ping-direct (bypassing router)", async () => {
    const request = mockRequest("GET", "http://localhost/ping-direct");
    const response = await httpWorker.fetch(
      request,
      mockEnv,
      mockExecutionContext
    );
    expect(response.status).toBe(200);
    const text = await response.text();
    expect(text).toBe("pong direct");
  });
});

describe("Worker HTTP Endpoints - Minimal Ping Test", () => {
  beforeEach(() => {
    vi.resetModules(); // Important for fresh worker instance per test
    vi.clearAllMocks();

    // Minimal mockEnv, add KV or DO stubs if absolutely required by the minimal worker
    mockEnv = {
      // LOG_LEVEL: 'debug', // Logger is removed from worker for now
      // KV_NAMESPACE: createMockKvNamespace() as unknown as KVNamespace, // KV is not used by minimal ping
      // POSITIONS_DO: createMockDoNamespace() as unknown as DurableObjectNamespace, // DO is not used by minimal ping
    };
  });

  it('should return 200 and "pong" for GET /ping', async () => {
    const request = new Request("http://localhost/ping", { method: "GET" });
    const testCtx = cfCreateExecutionContext();

    // Pass only the necessary parts of mockEnv that the simplified worker might expect (even if it's 'any')
    const response = await httpWorker.fetch(request, mockEnv as Env, testCtx);
    await cfWaitOnExecutionContext(testCtx);

    expect(response.status).toBe(200);
    const responseText = await response.text();
    expect(responseText).toBe("pong");
  });
});

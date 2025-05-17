/// <reference types="vitest/globals" />
import {
  describe,
  it,
  expect,
  vi,
  beforeEach,
  afterEach,
  type Mock,
  type MockInstance,
} from "vitest";

// Removed direct vi.mock("node:os") as a proper winston mock should make it unnecessary for this test file.

import * as winston from "winston"; // This import will be replaced by the mock
import { createLogger } from "../../src/utils/logger";
import type { LoggerInterface } from "../../src/types";

// Define LogEntry for this test file
interface LogEntry {
  level: string;
  message: string;
  [key: string]: any; // Allow other properties, linter might flag as 'any'
}

const mockWinstonLogger = {
  debug: vi.fn(),
  info: vi.fn(),
  warn: vi.fn(),
  error: vi.fn(),
  log: vi.fn(),
};

// Define a type for our mock format objects
interface MockFormatType {
  _formatName: string;
  options?: Record<string, any>; // Linter might complain
  // Workaround for linter misinterpreting Vitest's Mock type
  transform: any; // Was: Mock<[LogEntry], LogEntry | false>;
  combinedFormats?: MockFormatType[];
}

vi.mock("winston", () => {
  const createMockFormat = (
    name: string,
    options?: Record<string, any>
  ): MockFormatType => ({
    _formatName: name,
    options,
    // Workaround for linter misinterpreting Vitest's Mock type
    transform: vi.fn((info: LogEntry): LogEntry | false => info) as any, // Was: as Mock<[LogEntry], LogEntry | false>,
  });

  return {
    config: {
      npm: {
        levels: {
          error: 0,
          warn: 1,
          info: 2,
          http: 3,
          verbose: 4,
          debug: 5,
          silly: 6,
        },
        colors: {},
      },
    },
    createLogger: vi.fn(() => mockWinstonLogger as unknown as winston.Logger),
    transports: {
      Console: vi.fn().mockImplementation(() => ({})),
    },
    format: {
      combine: vi.fn(
        (...formatsArgs: MockFormatType[]): MockFormatType => ({
          _formatName: "combine",
          combinedFormats: formatsArgs,
          // Workaround for linter misinterpreting Vitest's Mock type
          transform: vi.fn((info: LogEntry): LogEntry | false => info) as any, // Was: as Mock<[LogEntry], LogEntry | false>,
        })
      ),
      timestamp: vi.fn((options?: Record<string, any>) =>
        createMockFormat("timestamp", options)
      ),
      printf: vi.fn((templateFn?: (info: LogEntry) => string) =>
        createMockFormat("printf", { templateFn })
      ),
      colorize: vi.fn((options?: Record<string, any>) =>
        createMockFormat("colorize", options)
      ),
      align: vi.fn(() => createMockFormat("align")),
      json: vi.fn(() => createMockFormat("json")),
    },
  };
});

describe("createLogger", () => {
  let originalNodeEnv: string | undefined;

  beforeEach(() => {
    vi.clearAllMocks();
    originalNodeEnv = process.env.NODE_ENV;
    process.env.NODE_ENV = undefined;
  });

  afterEach(() => {
    process.env.NODE_ENV = originalNodeEnv;
  });

  it("should create a logger instance", () => {
    const logger = createLogger();
    expect(logger).toBeDefined();
    expect(winston.createLogger).toHaveBeenCalledTimes(1);
  });

  it("should default to 'info' log level if not specified", () => {
    createLogger();
    expect(winston.createLogger).toHaveBeenCalledWith(
      expect.objectContaining({
        level: "info",
      })
    );
  });

  it("should use specified log level", () => {
    createLogger("debug");
    expect(winston.createLogger).toHaveBeenCalledWith(
      expect.objectContaining({
        level: "debug",
      })
    );
  });

  it("should use consoleFormat when NODE_ENV is not 'production'", () => {
    process.env.NODE_ENV = "development";
    createLogger();

    // Workaround for linter misinterpreting Vitest's Mock type
    const consoleMock = winston.transports.Console as any; // Was: Mock<[any?], any>;
    const consoleTransportOptions = consoleMock.mock.calls[0][0];
    const passedFormat = consoleTransportOptions.format as MockFormatType;

    expect(passedFormat._formatName).toBe("combine");
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "colorize")
    ).toBe(true);
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "timestamp")
    ).toBe(true);
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "align")
    ).toBe(true);
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "printf")
    ).toBe(true);
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "json")
    ).toBe(false);
  });

  it("should use jsonFormat when NODE_ENV is 'production'", () => {
    process.env.NODE_ENV = "production";
    createLogger();

    // Workaround for linter misinterpreting Vitest's Mock type
    const consoleMock = winston.transports.Console as any; // Was: Mock<[any?], any>;
    const consoleTransportOptions = consoleMock.mock.calls[0][0];
    const passedFormat = consoleTransportOptions.format as MockFormatType;

    expect(passedFormat._formatName).toBe("combine");
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "timestamp")
    ).toBe(true);
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "json")
    ).toBe(true);
    expect(
      passedFormat.combinedFormats?.some((f) => f._formatName === "colorize")
    ).toBe(false);
  });

  describe("LoggerInterface methods", () => {
    let logger: LoggerInterface;

    beforeEach(() => {
      process.env.NODE_ENV = undefined;
      logger = createLogger();
    });

    it("should call winston's debug method", () => {
      logger.debug("test debug", { data: 1 });
      expect(mockWinstonLogger.debug).toHaveBeenCalledWith("test debug", {
        data: 1,
      });
    });

    it("should call winston's info method", () => {
      logger.info("test info", { data: 2 });
      expect(mockWinstonLogger.info).toHaveBeenCalledWith("test info", {
        data: 2,
      });
    });

    it("should call winston's warn method", () => {
      logger.warn("test warn", { data: 3 });
      expect(mockWinstonLogger.warn).toHaveBeenCalledWith("test warn", {
        data: 3,
      });
    });

    it("should call winston's error method", () => {
      logger.error("test error", { data: 4 });
      expect(mockWinstonLogger.error).toHaveBeenCalledWith("test error", {
        data: 4,
      });
    });

    it("should call winston's log method for the generic log function", () => {
      logger.log("http", "test http log", { data: 5 });
      expect(mockWinstonLogger.log).toHaveBeenCalledWith(
        "http",
        "test http log",
        { data: 5 }
      );
    });

    it("should not have a functional addError method by default", () => {
      expect(logger.addError).toBeUndefined();
    });

    it("should not have a functional addContext method by default", () => {
      expect(logger.addContext).toBeUndefined();
    });

    it("should not have a functional http method by default", () => {
      expect(logger.http).toBeUndefined();
    });

    it("should not have a functional verbose method by default", () => {
      expect(logger.verbose).toBeUndefined();
    });

    it("should not have a functional silly method by default", () => {
      expect(logger.silly).toBeUndefined();
    });

    it("should not have a functional child method by default", () => {
      expect(logger.child).toBeUndefined();
    });
  });
});

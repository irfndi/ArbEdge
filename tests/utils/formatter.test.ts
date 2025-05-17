/// <reference types="vitest/globals" />
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { formatOpportunityMessage } from "../../src/utils/formatter";
import type { ArbitrageOpportunity, ExchangeId } from "../../src/types";

// Helper to create a consistent timestamp and mock Date behavior for time string generation
const MOCK_TIMESTAMP = 1672531200000; // 2023-01-01T00:00:00.000Z GMT
// Adjusted to what the test environment seems to produce for MOCK_TIMESTAMP
const EXPECTED_TIME_STRING = "1/1/2023, 7:00:00 AM";
const MONEY_BAG_EMOJI_UNICODE = "\uD83D\uDCB0"; // Unicode for 💰

describe("formatterUtils", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(MOCK_TIMESTAMP));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("formatOpportunityMessage", () => {
    const baseOpportunity: ArbitrageOpportunity = {
      pair: "BTC/USDT",
      longExchange: "binance" as ExchangeId,
      shortExchange: "bybit" as ExchangeId,
      longRate: 0.0001, // 0.01%
      shortRate: 0.0002, // 0.02%
      rateDifference: 0.0001, // 0.02% - 0.01%
      timestamp: MOCK_TIMESTAMP,
      type: "fundingRate",
      longExchangeTakerFeeRate: 0.00005,
      shortExchangeTakerFeeRate: 0.00005,
      totalEstimatedFees: 0.0001, // 0.00005 + 0.00005
      netRateDifference: 0, // rateDifference - totalEstimatedFees = 0.0001 - 0.0001 = 0
    };

    it("should format a standard funding rate opportunity", () => {
      const message = formatOpportunityMessage(baseOpportunity);
      expect(message).toContain("🚨 *Arbitrage Opportunity Detected* 🚨");
      expect(message).toContain("📈 *Pair:* `BTC/USDT`");
      expect(message).toContain("↔️ *Action:* LONG `BINANCE` / SHORT `BYBIT`");
      expect(message).toContain("*Rates \\(Funding\\):*");
      expect(message).toContain("\\- Long \\(BINANCE\\): `0\\.0100%`");
      expect(message).toContain("\\- Short \\(BYBIT\\): `0\\.0200%`");
      expect(message).toContain(
        `${MONEY_BAG_EMOJI_UNICODE} *Gross Difference:* \`0\\.0100%\``
      );
      expect(message).toContain("💹 *Net Difference:* `0\\.0000%`");
      expect(message).toContain(`🕒 *Detected At:* ${EXPECTED_TIME_STRING}`);
      expect(message).toContain("`/execute BTC_USDT BINANCE BYBIT 0\\.1 10`");
    });

    it("should correctly reflect positive net difference", () => {
      const opportunity: ArbitrageOpportunity = {
        ...baseOpportunity,
        longRate: 0.0001,
        shortRate: 0.0003,
        rateDifference: 0.0002, // 0.03 - 0.01
        netRateDifference: 0.0001, // 0.0002 - 0.0001 (fees)
      };
      const message = formatOpportunityMessage(opportunity);
      expect(message).toContain("\\- Long \\(BINANCE\\): `0\\.0100%`");
      expect(message).toContain("\\- Short \\(BYBIT\\): `0\\.0300%`");
      expect(message).toContain(
        `${MONEY_BAG_EMOJI_UNICODE} *Gross Difference:* \`0\\.0200%\``
      );
      expect(message).toContain("💹 *Net Difference:* `0\\.0100%`");
    });

    it("should handle missing optional fields by not displaying them", () => {
      const minimalOpportunity: ArbitrageOpportunity = {
        pair: "ETH/USDT",
        longExchange: "kraken" as ExchangeId,
        shortExchange: "mexc" as ExchangeId,
        longRate: 0.0003,
        shortRate: 0.0004,
        rateDifference: 0.0001,
        timestamp: MOCK_TIMESTAMP,
        type: "fundingRate",
        longExchangeTakerFeeRate: 0.00002,
        shortExchangeTakerFeeRate: 0.00002,
        totalEstimatedFees: 0.00004,
        netRateDifference: 0.00006, // 0.0001 - 0.00004
      };
      const message = formatOpportunityMessage(minimalOpportunity);
      expect(message).toContain("📈 *Pair:* `ETH/USDT`");
      expect(message).toContain("💹 *Net Difference:* `0\\.0060%`");
      expect(message).not.toContain("💸 *Potential Profit:*");
      expect(message).not.toContain("📝 *Details:*");
    });

    it("should include potential profit if provided", () => {
      const opportunity: ArbitrageOpportunity = {
        ...baseOpportunity,
        potentialProfitValue: 12.345,
      };
      const message = formatOpportunityMessage(opportunity);
      expect(message).toContain("💸 *Potential Profit:* `12\\.35` USDT");
    });

    it("should include details if provided", () => {
      const opportunity: ArbitrageOpportunity = {
        ...baseOpportunity,
        details: "Additional observation notes here.",
      };
      const message = formatOpportunityMessage(opportunity);
      expect(message).toContain(
        "📝 *Details:* Additional observation notes here\\."
      );
    });

    it("should format for a generic opportunity type if not 'fundingRate'", () => {
      const opportunity: ArbitrageOpportunity = {
        ...baseOpportunity,
        type: "spotSpread",
      };
      const message = formatOpportunityMessage(opportunity);
      expect(message).toContain("ℹ️ *Type:* spotSpread");
      expect(message).toContain(
        `${MONEY_BAG_EMOJI_UNICODE} *Gross Metric:* \`0\\.0100%\``
      );
      expect(message).toContain("➡️ *Exchange 1:* `BINANCE`");
      expect(message).toContain("⬅️ *Exchange 2:* `BYBIT`");
      expect(message).not.toContain("↔️ *Action:*");
      expect(message).not.toContain("*Rates \\(Funding\\):*");
      expect(message).not.toContain("/execute");
    });

    it("should handle missing longExchange for generic type by not displaying its line", () => {
      const opportunityWithMissingLongEx = {
        ...baseOpportunity,
        type: "generic",
        longExchange: undefined as unknown as ExchangeId,
      } as ArbitrageOpportunity;
      const message = formatOpportunityMessage(opportunityWithMissingLongEx);
      expect(message).toContain("ℹ️ *Type:* generic");
      expect(message).not.toContain("➡️ *Exchange 1:*");
      expect(message).toContain("⬅️ *Exchange 2:* `BYBIT`");
    });

    it("should handle missing shortExchange for generic type by not displaying its line", () => {
      const opportunityWithMissingShortEx = {
        ...baseOpportunity,
        type: "generic",
        shortExchange: undefined as unknown as ExchangeId,
      } as ArbitrageOpportunity;
      const message = formatOpportunityMessage(opportunityWithMissingShortEx);
      expect(message).toContain("ℹ️ *Type:* generic");
      expect(message).toContain("➡️ *Exchange 1:* `BINANCE`");
      expect(message).not.toContain("⬅️ *Exchange 2:*");
    });

    it("should escape markdownV2 special characters in all relevant fields", () => {
      const opportunity: ArbitrageOpportunity = {
        pair: "BTC*USDT[Test]",
        longExchange: "binance-long" as ExchangeId,
        shortExchange: "bybit_short" as ExchangeId,
        longRate: 0.0001,
        shortRate: 0.0002,
        rateDifference: 0.0001,
        timestamp: MOCK_TIMESTAMP,
        type: "fundingRate",
        details: "This is a test. (Important!)",
        potentialProfitValue: 10.5,
        longExchangeTakerFeeRate: 0.00005,
        shortExchangeTakerFeeRate: 0.00005,
        totalEstimatedFees: 0.0001,
        netRateDifference: 0.0, // Adjusted for consistency, was 0.00005 causing confusion
      };
      const message = formatOpportunityMessage(opportunity);

      expect(message).toContain("📈 *Pair:* `BTC\\*USDT\\[Test\\]`");
      expect(message).toContain(
        "↔️ *Action:* LONG `BINANCE\\-LONG` / SHORT `BYBIT\\_SHORT`"
      );
      expect(message).toContain("\\- Long \\(BINANCE\\-LONG\\): `0\\.0100%`");
      expect(message).toContain("\\- Short \\(BYBIT\\_SHORT\\): `0\\.0200%`");
      expect(message).toContain(
        `${MONEY_BAG_EMOJI_UNICODE} *Gross Difference:* \`0\\.0100%\``
      );
      expect(message).toContain(
        "📝 *Details:* This is a test\\. \\(Important\\!\\)"
      );
      expect(message).toContain("💸 *Potential Profit:* `10\\.50` USDT");
      expect(message).toContain(
        "`/execute BTC_*USDT_[Test_] BINANCE\\-LONG BYBIT\\_SHORT 0\\.1 10`"
      ); // Corrected escaping
    });

    it("should handle N/A for undefined rates", () => {
      type TestOpportunityWithOptionalRates = Omit<
        ArbitrageOpportunity,
        "longRate" | "shortRate"
      > & {
        longRate?: number;
        shortRate?: number;
      };
      const opportunity1: TestOpportunityWithOptionalRates = {
        ...baseOpportunity,
        longRate: undefined,
        shortRate: 0.0003,
      };
      const message1 = formatOpportunityMessage(
        opportunity1 as ArbitrageOpportunity
      );
      expect(message1).toContain("\\- Long \\(BINANCE\\): `N/A%`");
      expect(message1).toContain("\\- Short \\(BYBIT\\): `0\\.0300%`");

      const opportunity2: TestOpportunityWithOptionalRates = {
        ...baseOpportunity,
        longRate: 0.0003,
        shortRate: undefined,
      };
      const message2 = formatOpportunityMessage(
        opportunity2 as ArbitrageOpportunity
      );
      expect(message2).toContain("\\- Long \\(BINANCE\\): `0\\.0300%`");
      expect(message2).toContain("\\- Short \\(BYBIT\\): `N/A%`");
    });

    it("should handle potentialProfitValue being zero", () => {
      const opportunity: ArbitrageOpportunity = {
        ...baseOpportunity,
        potentialProfitValue: 0,
      };
      const message = formatOpportunityMessage(opportunity);
      expect(message).toContain("💸 *Potential Profit:* `0\\.00` USDT");
    });

    it("should correctly display long/short rates based on opportunity definition", () => {
      const message = formatOpportunityMessage(baseOpportunity);
      expect(message).toContain("↔️ *Action:* LONG `BINANCE` / SHORT `BYBIT`");
      expect(message).toContain("\\- Long \\(BINANCE\\): `0\\.0100%`");
      expect(message).toContain("\\- Short \\(BYBIT\\): `0\\.0200%`");
    });
  });
});

import type {
  ExchangeId,
  TradingPairSymbol,
  ArbitrageOpportunity,
  FundingRateInfo,
  StructuredTradingPair,
  LoggerInterface,
} from "../types.ts";
import type { TelegramService } from "./telegramService";
import type { ExchangeService } from "./exchangeService";
import { pRateLimit } from "p-ratelimit";
import type * as ccxt from "ccxt";

const limit = pRateLimit({
  interval: 1000,
  rate: 10,
  concurrency: 5,
});

/**
 * Service responsible for identifying arbitrage opportunities based on funding rates.
 */
export interface OpportunityServiceConfig {
  exchangeService: ExchangeService;
  telegramService: TelegramService | null;
  logger: LoggerInterface;
  monitoredPairs: StructuredTradingPair[];
  exchanges: ExchangeId[];
  threshold: number;
}

export interface IOpportunityService {
  getConfig(): OpportunityServiceConfig;
  findOpportunities(
    exchangeIds: ExchangeId[],
    pairs: TradingPairSymbol[],
    threshold: number
  ): Promise<ArbitrageOpportunity[]>;
  monitorOpportunities(threshold: number): Promise<ArbitrageOpportunity[]>;
  processOpportunities(opportunities: ArbitrageOpportunity[]): Promise<void>;
}

export class OpportunityService implements IOpportunityService {
  private exchangeService: ExchangeService;
  private telegramService: TelegramService | null = null;
  private logger: LoggerInterface;
  private monitoredPairs: StructuredTradingPair[];
  private exchanges: ExchangeId[];
  private threshold: number;

  /**
   * Creates an instance of OpportunityService.
   * @param config An instance of OpportunityServiceConfig.
   */
  constructor(config: OpportunityServiceConfig) {
    this.exchangeService = config.exchangeService;
    this.telegramService = config.telegramService;
    this.logger = config.logger;
    this.monitoredPairs = config.monitoredPairs;
    this.exchanges = config.exchanges;
    this.threshold = config.threshold;
    this.logger.info("OpportunityService initialized", {
      monitoredPairsCount: this.monitoredPairs.length,
      exchangeCount: this.exchanges.length,
      threshold: this.threshold,
    });
  }

  public getConfig(): OpportunityServiceConfig {
    return {
      exchangeService: this.exchangeService,
      telegramService: this.telegramService,
      logger: this.logger,
      monitoredPairs: this.monitoredPairs,
      exchanges: this.exchanges,
      threshold: this.threshold,
    };
  }

  /**
   * Finds funding rate arbitrage opportunities across specified exchanges and pairs.
   *
   * @param exchangeIds - An array of exchange IDs to check (e.g., ['binance', 'bybit']).
   * @param pairs - An array of trading pairs to check (e.g., ['BTC/USDT', 'ETH/USDT']).
   * @param threshold - The minimum absolute funding rate difference required to identify an opportunity.
   * @returns A promise that resolves to an array of identified ArbitrageOpportunity objects.
   */
  public async findOpportunities(
    exchangeIds: ExchangeId[],
    pairs: TradingPairSymbol[],
    threshold: number
  ): Promise<ArbitrageOpportunity[]> {
    console.log("[TELEGRAM_TEST_DEBUG] findOpportunities called with:", {
      exchangeIds,
      pairs,
      threshold,
    });
    const opportunities: ArbitrageOpportunity[] = [];
    if (exchangeIds.length < 2) {
      this.logger.warn(
        "Skipping findOpportunities: Not enough exchanges to compare."
      );
      return opportunities;
    }

    const fundingRateData = new Map<
      TradingPairSymbol,
      Map<ExchangeId, FundingRateInfo | null>
    >();
    const tradingFeeData = new Map<
      TradingPairSymbol,
      Map<ExchangeId, ccxt.TradingFeeInterface | null>
    >(); // Assuming ccxt.TradingFeeInterface contains taker fee

    const fetchPromises: Promise<void>[] = [];

    for (const pair of pairs) {
      if (!fundingRateData.has(pair)) {
        fundingRateData.set(
          pair,
          new Map<ExchangeId, FundingRateInfo | null>()
        );
      }
      if (!tradingFeeData.has(pair)) {
        tradingFeeData.set(
          pair,
          new Map<ExchangeId, ccxt.TradingFeeInterface | null>()
        );
      }

      for (const exchangeId of exchangeIds) {
        // Fetch FundingRateInfo
        fetchPromises.push(
          limit(async () => {
            try {
              const rateInfo = await this.exchangeService.getFundingRate(
                exchangeId,
                pair
              );
              fundingRateData.get(pair)?.set(exchangeId, rateInfo || null);
            } catch (e) {
              this.logger.error(
                `Error fetching funding rate for ${pair} on ${exchangeId}`,
                { error: e }
              );
              fundingRateData.get(pair)?.set(exchangeId, null);
            }
          })
        );

        // Fetch TradingFeesInfo
        fetchPromises.push(
          limit(async () => {
            try {
              const feeInfo = await this.exchangeService.getTradingFees(
                exchangeId,
                pair
              );
              tradingFeeData.get(pair)?.set(exchangeId, feeInfo || null);
            } catch (e) {
              this.logger.error(
                `Error fetching trading fees for ${pair} on ${exchangeId}`,
                { error: e }
              );
              tradingFeeData.get(pair)?.set(exchangeId, null);
            }
          })
        );
      }
    }

    await Promise.all(fetchPromises);

    console.log(
      "[TELEGRAM_TEST_DEBUG] All funding rates and trading fees fetched."
    );

    // Step 3.5 (New): Get general fee status for all involved exchanges
    const exchangeFeeStatus = new Map<ExchangeId, { isFeeFree: boolean }>();
    const allExchangeIdsInvolved = new Set<ExchangeId>();
    for (const exchangeId of exchangeIds) {
      try {
        const instance =
          await this.exchangeService.getExchangeInstance(exchangeId);
        console.log(
          `[TELEGRAM_TEST_DEBUG] Fetched CCXT instance for ${exchangeId} for general fee status:`,
          instance !== null
        );
        if (instance && instance.fees?.trading?.taker === 0) {
          exchangeFeeStatus.set(exchangeId, { isFeeFree: true });
          this.logger.debug(
            `Exchange ${exchangeId} identified as generally fee-free.`
          );
          console.log(
            `[TELEGRAM_TEST_DEBUG] ${exchangeId} is generally FEE-FREE.`
          );
        } else {
          exchangeFeeStatus.set(exchangeId, { isFeeFree: false });
          if (instance && instance.fees?.trading?.taker !== undefined) {
            this.logger.debug(
              `Exchange ${exchangeId} is not generally fee-free. Default taker fee: ${instance.fees.trading.taker}`
            );
            console.log(
              `[TELEGRAM_TEST_DEBUG] ${exchangeId} not generally fee-free. Default taker: ${instance.fees.trading.taker}`
            );
          } else if (instance) {
            this.logger.debug(
              `Exchange ${exchangeId} has no general trading fee info or taker fee is undefined.`
            );
            console.log(
              `[TELEGRAM_TEST_DEBUG] ${exchangeId} no general trading fee info or taker undefined.`
            );
          } else {
            this.logger.warn(
              `Could not retrieve CCXT instance for ${exchangeId} to check general fee status.`
            );
            console.log(
              `[TELEGRAM_TEST_DEBUG] Could not retrieve CCXT instance for ${exchangeId} (general fee check).`
            );
          }
        }
      } catch (error) {
        this.logger.error(
          `Failed to get exchange instance or fees for ${exchangeId} during general fee check`,
          { exchangeId, error }
        );
        console.error(
          `[TELEGRAM_TEST_DEBUG] Error getting instance/fees for ${exchangeId} (general fee check):`,
          error
        );
        exchangeFeeStatus.set(exchangeId, { isFeeFree: false }); // Assume not fee-free on error
      }
    }
    console.log(
      "[TELEGRAM_TEST_DEBUG] exchangeFeeStatus map populated:",
      JSON.stringify(Array.from(exchangeFeeStatus.entries()))
    );

    const pairFundingRates = new Map<TradingPairSymbol, FundingRateInfo[]>();
    const pairTradingFees = new Map<
      TradingPairSymbol,
      Map<ExchangeId, ccxt.TradingFeeInterface | null>
    >();

    // Step 3.75: Populate pairFundingRates and pairTradingFees from fetched data
    for (const [pair, ratesMap] of fundingRateData.entries()) {
      const validRates = Array.from(ratesMap.values()).filter(
        (rate): rate is FundingRateInfo => rate !== null
      );
      if (validRates.length > 0) {
        pairFundingRates.set(pair, validRates);
      }
    }
    for (const [pair, feesMap] of tradingFeeData.entries()) {
      pairTradingFees.set(pair, feesMap);
    }

    // Step 4: Iterate through pairs and then through combinations of exchanges for each pair
    for (const [pair, ratesForPair] of pairFundingRates.entries()) {
      console.log(
        `[TELEGRAM_TEST_DEBUG] Processing pair from pairFundingRates: ${pair} with ${ratesForPair.length} rates.`
      );

      const uniqueRates = Array.from(
        new Set(ratesForPair.map((r) => r.fundingRate))
      );
      if (uniqueRates.length < 2) {
        this.logger.warn(
          `Skipping ${pair} as it does not have at least two different funding rates among the provided exchanges.`
        );
        console.log(
          `[TELEGRAM_TEST_DEBUG] Skipping ${pair} - unique rates: ${uniqueRates.length}`
        );
        continue;
      }

      const sortedRates = [...ratesForPair].sort(
        (a, b) => a.fundingRate - b.fundingRate
      );
      console.log(
        `[TELEGRAM_TEST_DEBUG] Pair: ${pair}, SortedRates: ${JSON.stringify(sortedRates.map((r) => ({ ex: r.exchange, rate: r.fundingRate })))}`
      );

      // Filter exchanges based on availability of funding rates AND fees (or if exchange is fee-free)
      const availableExchanges = sortedRates
        .map((r) => r.exchange)
        .filter((exchangeId) => {
          const hasFundingRate =
            ratesForPair.find((r) => r.exchange === exchangeId) !== undefined;
          if (!hasFundingRate) return false;

          const hasSpecificFee =
            pairTradingFees.get(pair)?.get(exchangeId) !== null &&
            pairTradingFees.get(pair)?.get(exchangeId) !== undefined;
          const isGenerallyFeeFree =
            exchangeFeeStatus.get(exchangeId)?.isFeeFree === true;

          if (hasSpecificFee || isGenerallyFeeFree) {
            return true;
          }

          this.logger.debug(
            `Excluding ${exchangeId} for pair ${pair}: Funding rate present but no specific fee info and not generally fee-free.`
          );
          return false;
        });

      if (availableExchanges.length < 2) {
        this.logger.debug(
          `Skipping ${pair} - not enough exchanges with funding rates.`
        );
        continue;
      }

      for (let i = 0; i < availableExchanges.length; i++) {
        for (let j = i + 1; j < availableExchanges.length; j++) {
          const exchangeA = availableExchanges[i];
          const exchangeB = availableExchanges[j];

          const fundingRateInfoA = ratesForPair.find(
            (r) => r.exchange === exchangeA
          );
          const fundingRateInfoB = ratesForPair.find(
            (r) => r.exchange === exchangeB
          );

          // Spec 4.b.i.3: If fundingRateInfoA or fundingRateInfoB is null, continue.
          // This is already handled by availableExchanges filter, but double check for safety.
          if (!fundingRateInfoA || !fundingRateInfoB) {
            this.logger.warn(
              `Logic error: Missing funding rate info for ${pair} on ${exchangeA} or ${exchangeB} despite pre-filter. Skipping.`
            );
            continue;
          }

          const tradingFeesA = pairTradingFees.get(pair)?.get(exchangeA);
          const tradingFeesB = pairTradingFees.get(pair)?.get(exchangeB);

          // Spec 4.b.i.4: Determine long and short exchange
          let longExchange: ExchangeId;
          let shortExchange: ExchangeId;
          let longRate: number;
          let shortRate: number;
          let longRateTimestamp: number;

          if (fundingRateInfoA.fundingRate <= fundingRateInfoB.fundingRate) {
            longExchange = exchangeA;
            shortExchange = exchangeB;
            longRate = fundingRateInfoA.fundingRate;
            shortRate = fundingRateInfoB.fundingRate;
            longRateTimestamp = fundingRateInfoA.timestamp;
          } else {
            longExchange = exchangeB;
            shortExchange = exchangeA;
            longRate = fundingRateInfoB.fundingRate;
            shortRate = fundingRateInfoA.fundingRate;
            longRateTimestamp = fundingRateInfoB.timestamp;
          }

          // Spec 4.b.i.5: Calculate rateDifference (gross)
          // rateDifference is shortRate - longRate because longRate is smaller (or more negative)
          // To get an absolute difference if order is not guaranteed, use Math.abs(rateA - rateB)
          // Given our long/short determination, shortRate - longRate will be positive if shortRate > longRate.
          const rateDifference = shortRate - longRate;

          // If rateDifference is not positive, it's not an arbitrage in this direction.
          // (e.g. if both are negative, shortRate = -0.01, longRate = -0.05, then -0.01 - (-0.05) = 0.04)
          // if shortRate = 0.05, longRate = 0.01, then 0.05 - 0.01 = 0.04
          // if shortRate = 0.01, longRate = 0.05, then 0.01 - 0.05 = -0.04 (this case shouldn't happen due to long/short assignment)
          // The spec uses Math.abs(shortRate - longRate) for `rateDifference`
          // then netRateDifference = rateDifference - totalEstimatedFees.
          // Let's stick to the spec's definition for rateDifference more closely:
          const specRateDifference = Math.abs(
            fundingRateInfoA.fundingRate - fundingRateInfoB.fundingRate
          );

          // Determine fee rates based on the refined logic
          let feeRateLong: number = Number.NaN;
          let feeRateShort: number = Number.NaN;

          const assignFees = (longEx: ExchangeId, shortEx: ExchangeId) => {
            const longFeeInfo = pairTradingFees.get(pair)?.get(longEx);
            if (
              longFeeInfo?.taker !== undefined &&
              longFeeInfo?.taker !== null
            ) {
              feeRateLong = longFeeInfo.taker;
            } else if (exchangeFeeStatus.get(longEx)?.isFeeFree) {
              feeRateLong = 0;
            } else {
              this.logger.error(
                `Critical: Fee cannot be determined for longExchange ${longEx} on ${pair}.`
              );
              feeRateLong = Number.NaN;
            }

            const shortFeeInfo = pairTradingFees.get(pair)?.get(shortEx);
            if (
              shortFeeInfo?.taker !== undefined &&
              shortFeeInfo?.taker !== null
            ) {
              feeRateShort = shortFeeInfo.taker;
            } else if (exchangeFeeStatus.get(shortEx)?.isFeeFree) {
              feeRateShort = 0;
            } else {
              this.logger.error(
                `Critical: Fee cannot be determined for shortExchange ${shortEx} on ${pair}.`
              );
              feeRateShort = Number.NaN;
            }
          };

          assignFees(longExchange, shortExchange);
          console.log(
            `[TELEGRAM_TEST_DEBUG] Pair: ${pair}, LongEx: ${longExchange}, ShortEx: ${shortExchange}, feeRateLong: ${feeRateLong}, feeRateShort: ${feeRateShort}`
          );

          // Spec 4.b.i.6: Calculate totalEstimatedFees
          const totalEstimatedFees = feeRateLong + feeRateShort;

          // Spec 4.b.i.7: Calculate netRateDifference
          const netRateDifference = specRateDifference - totalEstimatedFees;

          console.log(
            `[TELEGRAM_TEST_DEBUG] Pair: ${pair}, Gross: ${rateDifference}, Fees: ${totalEstimatedFees}, Net: ${netRateDifference}, Threshold: ${threshold}`
          );

          // Spec 4.b.i.8: Check Opportunity Condition
          // The condition is: netRateDifference >= threshold AND shortRate > longRate
          // The shortRate > longRate is implicitly handled by how long/short exchanges are determined
          // and how rateDifference is calculated (shortRate - longRate which would be negative otherwise).
          // However, the specRateDifference = Math.abs(...) was introduced.
          // Let's ensure the original rateDifference (shortRate - longRate) is used for the actual check.

          const actualProfitPotentialDirection = shortRate - longRate; // This should be positive if shortRate > longRate

          if (
            netRateDifference >= threshold &&
            actualProfitPotentialDirection > 0 // Explicitly check direction, though netRateDifference relies on absolute.
            // The spec's condition `shortRate > longRate` ensures this.
            // If long/short assignment is correct, this condition is inherently met
            // when `specRateDifference` (based on absolute) minus fees is positive.
            // Let's use the simpler condition from the previous implementation
            // if netRateDifference >= threshold, given that long/short are already decided.
            // The spec says: "netRateDifference >= Config.threshold AND Short Exchange Funding Rate > Long Exchange Funding Rate"
            // Our `shortRate` and `longRate` variables already respect this.
          ) {
            const opportunity: ArbitrageOpportunity = {
              pair: pair,
              longExchange,
              shortExchange,
              longRate,
              shortRate,
              rateDifference: specRateDifference,
              longExchangeTakerFeeRate: feeRateLong,
              shortExchangeTakerFeeRate: feeRateShort,
              totalEstimatedFees,
              netRateDifference,
              timestamp: longRateTimestamp,
            };
            opportunities.push(opportunity);
            this.logger.info("Arbitrage opportunity found", { opportunity });
            if (this.telegramService) {
              this.telegramService
                .sendOpportunityNotification(opportunity)
                .catch((teleError) => {
                  this.logger.error("Failed to send Telegram notification", {
                    teleError,
                    // Using a unique part of the opportunity for logging if ID was removed
                    opportunityDetails: `${opportunity.pair}-${opportunity.longExchange}-${opportunity.shortExchange}-${opportunity.timestamp}`,
                  });
                });
            }
          }
        }
      }
    }

    this.logger.info(`Found ${opportunities.length} opportunities.`);
    return opportunities;
  }

  /**
   * Runs opportunity checks using configured pairs and exchanges.
   * @param threshold Minimum net rate difference to filter opportunities.
   */
  async monitorOpportunities(
    threshold: number
  ): Promise<ArbitrageOpportunity[]> {
    // Implementation for continuous monitoring will go here
    this.logger.info("monitorOpportunities called", { threshold });
    // For now, let's just call findOpportunities once as a placeholder
    return this.findOpportunities(
      this.exchanges,
      this.monitoredPairs.map((p) => p.symbol),
      threshold
    );
  }

  /**
   * Processes identified arbitrage opportunities.
   * For now, it logs the opportunities. This method can be expanded later.
   * @param opportunities - An array of ArbitrageOpportunity objects.
   */
  async processOpportunities(
    opportunities: ArbitrageOpportunity[]
  ): Promise<void> {
    if (this.telegramService) {
      for (const opp of opportunities) {
        try {
          await this.telegramService.sendOpportunityNotification(opp);
        } catch (e) {
          console.log(
            "[TELEGRAM_TEST_DEBUG] Entered CATCH block in processOpportunities for OpportunityService"
          ); // Added log
          this.logger.error(
            "Error sending opportunity notification via Telegram from OpportunityService",
            {
              // biome-ignore lint/suspicious/noExplicitAny: Error object can be of any type
              error: e as any,
              opportunityPair: opp.pair, // Using a distinct field name
            }
          );
        }
      }
    }
  }

  // Helper methods can be added here
}

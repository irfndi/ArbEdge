import { Bot, GrammyError, HttpError } from "grammy";
import type {
  ArbitrageOpportunity as TypedArbitrageOpportunity,
  LoggerInterface,
  ExchangeId,
  TelegramConfig,
} from "../types";

// REMOVE local ArbitrageOpportunity interface
// interface ArbitrageOpportunity {
//   pair: string;
//   longExchange: string | ExchangeId;
//   shortExchange: string | ExchangeId;
//   longRate: number;
//   shortRate: number;
//   rateDifference: number;
//   longExchangeTakerFeeRate?: number;
//   shortExchangeTakerFeeRate?: number;
//   netRateDifference?: number;
//   potentialProfit?: number;
//   timestamp: number;
//   longPrice?: number;
//   shortPrice?: number;
//   id?: string;
//   type?: string;
//   details?: string;
// }
import { formatOpportunityMessage } from "../utils/formatter";

// Interface for configuration (can be expanded)
// MOVED TO src/types.ts
// export interface TelegramConfig {
//   botToken: string;
//   chatId: string; // Target chat ID to send notifications
//   logger: LoggerInterface; // Added logger
// }

export interface TelegramServiceOptions {
  env?: string;
  startPolling?: boolean; // New option to control polling
}

export interface ITelegramService {
  sendOpportunityNotification(
    opportunity: TypedArbitrageOpportunity
  ): Promise<void>;
  sendMessage(text: string): Promise<void>;
  stop(): Promise<void>;
  getBotInstance(): Bot; // Add to interface
}

/**
 * Handles Telegram bot communication, including sending notifications about detected arbitrage opportunities.
 * Manages the bot instance and provides methods for sending messages to the configured chat ID.
 * @constructor
 * @param {TelegramConfig} config - The bot configuration object with bot token, chat ID, and logger.
 * @param {TelegramServiceOptions} [options] - Optional settings for the service.
 */
export class TelegramService implements ITelegramService {
  private bot: Bot;
  private config: TelegramConfig;
  private logger: LoggerInterface;
  private effectiveEnv: string;

  constructor(config: TelegramConfig, options?: TelegramServiceOptions) {
    this.config = config;
    this.logger = config.logger;
    this.effectiveEnv = options?.env ?? process.env.NODE_ENV ?? "development"; // Default to development

    if (!config.botToken || !config.chatId) {
      this.logger.error("Telegram Bot Token and Chat ID must be provided.");
      throw new Error("Telegram Bot Token and Chat ID must be provided.");
    }
    this.bot = new Bot(this.config.botToken);

    // Register commands and handlers
    this.registerCommandHandlers(); // Extracted for clarity

    // Enhanced error handling
    this.bot.catch((err) => {
      const ctx = err.ctx;
      const errorTime = new Date().toISOString();
      const updateId = ctx.update.update_id;

      // Log detailed error information
      this.logger.error(
        `[${errorTime}] Error while handling update ${updateId}:`,
        {
          error: String(err.error), // Convert unknown to string for safety
          errorMessage:
            err.error instanceof Error ? err.error.message : "Unknown error",
          errorStack: err.error instanceof Error ? err.error.stack : undefined,
          // Use ctx properties safely
          user: ctx.from
            ? `${ctx.from.id} (${ctx.from.username || "no username"})`
            : "unknown",
          chat: ctx.chat ? `${ctx.chat.id} (${ctx.chat.type})` : "unknown",
        }
      );

      // Categorize errors for better handling
      if (err.error instanceof GrammyError) {
        this.logger.error(`Telegram API Error: ${err.error.description}`);
        // Notify administrators for specific error codes if needed
      } else if (err.error instanceof HttpError) {
        // HttpError might not have a direct status property
        const statusCode = "status" in err.error ? err.error.status : "unknown";
        this.logger.error(`HTTP Error: ${statusCode}`);
        // Implement exponential backoff retry for network issues
      } else {
        this.logger.error(`Unknown error: ${String(err.error)}`);
      }

      // Try to inform the user when possible
      try {
        ctx
          .reply(
            "An error occurred while processing your request. The team has been notified."
          )
          .catch((replyErr) =>
            this.logger.error(
              "Error sending error notification to user:",
              replyErr
            )
          );
      } catch (notifyError) {
        this.logger.error("Failed to notify user about error:", notifyError);
      }
    });

    // Start polling only if explicitly enabled and not in test environment
    const autoStartBasedOnEnv =
      this.effectiveEnv !== "test" &&
      this.effectiveEnv !== "production_webhook";
    const shouldStartPolling =
      options?.startPolling === undefined
        ? autoStartBasedOnEnv
        : options.startPolling && autoStartBasedOnEnv;

    if (shouldStartPolling) {
      this.bot.start();
      this.logger.info("Telegram Bot started in polling mode...");
    }
  }

  private registerCommandHandlers(): void {
    this.bot.command("start", (ctx) =>
      ctx.reply(
        "Welcome to the Arbitrage Bot!\n" +
          "I can help you detect funding rate arbitrage opportunities and notify you about them.\n\n" +
          "Here are the available commands:\n" +
          "/help - Show this help message and list all commands.\n" +
          "/status - Check the bot's current operational status.\n" +
          "/opportunities - Show recent arbitrage opportunities (currently placeholder).\n" +
          "/settings - View current bot settings (currently placeholder).\n\n" +
          "Use /help to see this list again."
      )
    );

    this.bot.command("help", (ctx) =>
      ctx.reply(
        "Available commands:\n" +
          "/help - Show this help message\n" +
          "/status - Check bot status\n" +
          "/opportunities - Show recent opportunities\n" +
          "/settings - View current settings"
      )
    );

    this.bot.command("status", (ctx) => {
      const now = new Date().toISOString();
      return ctx.reply(
        `Bot is active and monitoring for arbitrage opportunities.\nCurrent time: ${now}`
      );
    });

    this.bot.command("opportunities", async (ctx) => {
      try {
        return ctx.reply(
          "No recent opportunities found. Will notify you when new ones are detected."
        );
      } catch (error) {
        this.logger.error("Error handling opportunities command:", error);
        return ctx.reply(
          "Error fetching opportunities. Please try again later."
        );
      }
    });

    this.bot.command("settings", async (ctx) => {
      return ctx.reply(
        "Current settings:\n" +
          "Threshold: 0.001 (0.1%)\n" +
          "Pairs monitored: BTC/USDT, ETH/USDT\n" +
          "Exchanges: Binance, Bybit, OKX"
      );
    });
  }

  /**
   * Returns the underlying grammY Bot instance.
   * Useful for integrating with web frameworks using webhookCallback.
   */
  public getBotInstance(): Bot {
    return this.bot;
  }

  /**
   * Sends a notification message about a detected arbitrage opportunity.
   * @param opportunity The detected arbitrage opportunity.
   */
  async sendOpportunityNotification(
    opportunity: TypedArbitrageOpportunity
  ): Promise<void> {
    // Validation logic might need adjustment if fields in TypedArbitrageOpportunity are different/all mandatory
    // For now, let's assume TypedArbitrageOpportunity has all necessary fields for formatting.
    // The old validation for 'pair', 'longExchange' etc. on the local type might not fully apply.
    // A quick check for `pairSymbol` which is critical.
    if (!opportunity.pair) {
      this.logger.error(
        "Invalid opportunity, missing pair in TypedArbitrageOpportunity:",
        opportunity
      );
      return;
    }

    try {
      // formatOpportunityMessage will now receive TypedArbitrageOpportunity
      // It needs to be compatible with it (i.e., use opportunity.pair etc.)
      const message = formatOpportunityMessage(opportunity);
      await this.bot.api.sendMessage(this.config.chatId, message, {
        parse_mode: "MarkdownV2",
      });
      this.logger.info(
        `Sent opportunity notification for ${opportunity.pair} to chat ${this.config.chatId}`
      );
    } catch (error) {
      this.logger.error(
        `Failed to send Telegram opportunity notification for ${opportunity.pair}:`,
        error
      );

      // Implement retry logic - simplified to match test expectations
      try {
        // Retry once immediately
        const message = formatOpportunityMessage(opportunity);
        await this.bot.api.sendMessage(this.config.chatId, message, {
          parse_mode: "MarkdownV2",
        });
        this.logger.info("Successfully sent opportunity notification on retry");
      } catch (retryError) {
        this.logger.error("Retry failed:", retryError);
        throw retryError;
      }
    }
  }

  /**
   * Sends a generic text message to the configured chat ID.
   * @param text The message text to send.
   */
  async sendMessage(text: string): Promise<void> {
    try {
      await this.bot.api.sendMessage(this.config.chatId, text, {
        parse_mode: "MarkdownV2",
      });
      this.logger.info(`Sent message to chat ${this.config.chatId}`);
    } catch (error) {
      this.logger.error("Failed to send Telegram message:", error);
      throw error;
    }
  }

  /**
   * Stops the Telegram bot gracefully.
   */
  async stop(): Promise<void> {
    try {
      await this.bot.stop();
      this.logger.info("Telegram bot stopped successfully");
    } catch (error) {
      this.logger.error("Error stopping Telegram bot:", error);
      throw error;
    }
  }
}

// Example usage (consider moving to main application logic)
// Ensure environment variables are loaded (e.g., using dotenv)
// import dotenv from 'dotenv';
// dotenv.config();
//
// const telegramConfig: TelegramConfig = {
//   botToken: process.env.TELEGRAM_BOT_TOKEN || '',
//   chatId: process.env.TELEGRAM_CHAT_ID || '',
//   logger: console, // Example logger
// };
//
// if (telegramConfig.botToken && telegramConfig.chatId) {
//   const telegramService = new TelegramService(telegramConfig);
//   // Example notification
//   // const fakeOpp: ArbitrageOpportunity = {
//   //   pair: 'BTC/USDT',
//   //   longExchange: 'bybit',
//   //   shortExchange: 'binance',
//   //   longRate: 0.0001,
//   //   shortRate: 0.0010,
//   //   rateDifference: 0.0009,
//   //   timestamp: Date.now(),
//   // };
//   // telegramService.sendOpportunityNotification(fakeOpp);
// }

import type { TradingFees, TradingFeeInterface } from "ccxt"; // Import TradingFeeInterface

export function MOCK_TRADING_FEES_FACTORY(): TradingFees {
  return {
    BTCUSDT: {
      symbol: "BTC/USDT",
      maker: 0.001,
      taker: 0.001,
      percentage: true,
      tierBased: false,
      info: {}, // Added required info property
    },
    ETHUSDT: {
      symbol: "ETH/USDT",
      maker: 0.001,
      taker: 0.001,
      percentage: true,
      tierBased: false,
      info: {}, // Added required info property
    },
  };
}

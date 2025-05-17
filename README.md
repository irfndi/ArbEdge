![CodeRabbit Pull Request Reviews](https://img.shields.io/coderabbit/prs/github/irfndi/ArbEdge?utm_source=oss&utm_medium=github&utm_campaign=irfndi%2FArbEdge&labelColor=171717&color=FF570A&link=https%3A%2F%2Fcoderabbit.ai&label=CodeRabbit+Reviews)

# ArbEdge: Crypto Arbitrage Opportunity Detector

## 🌟 Description

ArbEdge is a sophisticated trading bot designed to identify and capitalize on arbitrage opportunities across various cryptocurrency exchanges. It focuses on funding rate arbitrage, monitoring real-time data from multiple exchanges to find profitable trades based on differences in funding rates for perpetual contracts.

The core service, `OpportunityService`, fetches funding rates and trading fees, calculates potential profit margins, and can notify users or other services about identified opportunities.

## ✨ Key Features

Based on `OpportunityServiceSpec.md` and `PRD.MD`:

- **Multi-Exchange Support**: Integrates with multiple cryptocurrency exchanges (e.g., Binance, Bybit, OKX, Kraken, Gate.io) via the `ccxt` library.
- **Funding Rate Arbitrage**:
  - Fetches real-time funding rates for specified trading pairs across configured exchanges.
  - Fetches taker fee rates for these pairs.
  - Identifies opportunities where a significant difference in funding rates (long on one exchange, short on another) exists.
  - Calculates net rate difference after accounting for estimated trading fees.
- **Opportunity Identification & Logging**:
  - Compares net rate differences against a configurable threshold.
  - Logs identified opportunities with detailed information (pair, exchanges, rates, fees, net profit).
- **Data Persistence (Conceptual)**: Designed with KV Namespace in mind for storing/caching fetched data like funding rates and trading fees to optimize API usage and performance.
- **Notification System (Conceptual)**: Includes a `TelegramService` for sending notifications about identified opportunities.
- **Configurable Monitoring**: Allows specifying which trading pairs and exchanges to monitor.
- **Resilient Data Fetching**: Incorporates rate limiting (`p-ratelimit`) for API calls to exchanges.
- **Structured Logging**: Uses a logger interface for consistent and informative logs.

## 🛠️ Tech Stack

- **Runtime**: Node.js
- **Language**: TypeScript
- **Package Manager**: pnpm
- **Testing**: Vitest (Unit & Integration Tests), Codecov (Coverage Reporting)
- **Core Trading Logic**: `ccxt` library for exchange interactions
- **Deployment**: Cloudflare Workers
- **Linting/Formatting**: Biome (as inferred from previous interactions)
- **CI/CD**: GitHub Actions

## 📂 Project Structure

```
ArbEdge/
├── .github/                # GitHub Actions workflows
│   └── workflows/
│       └── ci.yml
├── .vscode/                # VSCode settings (e.g., for Biome formatter)
├── Docs/                   # Project documentation
│   ├── PRD.MD              # Product Requirements Document
│   └── OpportunityServiceSpec.md # Specification for OpportunityService
├── src/                    # Source code
│   ├── services/           # Core services (OpportunityService, ExchangeService, TelegramService)
│   ├── utils/              # Utility functions
│   └── types.ts            # TypeScript type definitions
├── tests/                  # Test files
│   └── services/
│       └── opportunityService.test.ts
├── .env.example            # Example environment variables
├── .gitignore
├── biome.json              # Biome linter/formatter configuration
├── package.json
├── pnpm-lock.yaml
├── tsconfig.json
└── README.md               # This file
```

## 🚀 Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (version 18.x or later recommended)
- [pnpm](https://pnpm.io/installation)

### Installation

1.  **Clone the repository:**

    ```bash
    git clone <repository-url>
    cd ArbEdge
    ```

2.  **Install dependencies:**

    ```bash
    pnpm install
    ```

3.  **Set up environment variables:**
    Copy the `.env.example` file to a new file named `.env` and fill in the necessary API keys and configuration details.
    ```bash
    cp .env.example .env
    ```
    (Note: Actual environment variables will depend on the specific exchange integrations and services like Telegram.)

## 💻 Development

### Linting

To check for linting errors and formatting issues:

```bash
pnpm run lint
```

To automatically fix fixable linting and formatting issues:

```bash
pnpm run lint:fix # Or equivalent Biome command, e.g., pnpm exec biome check --apply
```

_(Adjust the `lint:fix` script in `package.json` if it's different or uses a specific Biome command)_

### Running Tests

To run all tests:

```bash
pnpm test
```

To run tests with coverage:

```bash
pnpm run test:coverage
```

Coverage reports are uploaded to Codecov via the CI pipeline.

## ⚙️ Configuration

The application likely requires configuration through environment variables, especially for:

- API keys and secrets for different exchanges.
- Telegram Bot Token and Chat ID for notifications.
- Parameters for the `OpportunityService` like monitoring thresholds, pairs, and exchanges (though some might be hardcoded or configurable via other means).

Refer to `.env.example` for a template of required environment variables.

## ☁️ Deployment

This project is configured for deployment to [Cloudflare Workers](https://workers.cloudflare.com/). The deployment process is handled by the `pnpm run deploy` script, which is typically triggered by the GitHub Actions CI/CD pipeline on pushes to the `development` branch.

The CI pipeline in `.github/workflows/ci.yml` automates testing, security analysis, and deployment.

## 🤝 Contributing

Contributions are welcome! Please follow the standard fork-and-pull-request workflow. Ensure your code adheres to the linting rules and all tests pass before submitting a pull request.

_(Further details can be added here, such as coding style guidelines or a Code of Conduct.)_

## 📄 License

_(Specify license, e.g., MIT, Apache 2.0, or leave as "Proprietary" if not open source.)_
This project is currently unlicensed/proprietary.

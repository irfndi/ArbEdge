import { defineWorkersConfig } from "@cloudflare/vitest-pool-workers/config";
// import path from "node:path"; // No longer needed for alias

export default defineWorkersConfig({
  test: {
    globals: true,
    poolOptions: {
      workers: {
        wrangler: { configPath: "./wrangler.toml" },
        // Miniflare specific options
        miniflare: {
          // You can add other Miniflare options here if needed
          // liveReload: true,
        },
      },
    },
    deps: {
      optimizer: {
        ssr: {
          enabled: true,
          include: ["winston"],
        },
      },
    },
    coverage: {
      provider: "istanbul",
      reporter: ["text", "lcov"],
      thresholds: {
        statements: 95,
        branches: 95,
        functions: 95,
        lines: 95,
      },
    },
    // resolve: { // Alias was not effective for the worker environment
    //   alias: {
    //     "node:os": path.resolve(__dirname, "tests/mocks/nodeOsShim.ts"),
    //   },
    // },
  },
});

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: { timeout: 15_000 },
  reporter: "list",
  use: {
    baseURL: "http://127.0.0.1:5173",
    viewport: { width: 1440, height: 1080 },
    video: { mode: "on", size: { width: 1440, height: 1080 } },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    launchOptions: { slowMo: process.env.DEMO ? 400 : 0 },
  },
  webServer: [
    {
      command: "cargo run --locked --manifest-path ../Cargo.toml",
      url: "http://127.0.0.1:3000/health",
      timeout: 120_000,
      env: {
        WRITE_TOKEN: "local-homework-token",
        RUST_LOG: "market_data=info",
      },
      reuseExistingServer: !process.env.CI,
    },
    {
      command: "npm run dev",
      url: "http://127.0.0.1:5173",
      reuseExistingServer: !process.env.CI,
    },
  ],
});

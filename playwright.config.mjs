import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/ui",
  workers: 1,
  use: { baseURL: "http://127.0.0.1:5175", viewport: { width: 420, height: 600 }, screenshot: "only-on-failure",
    launchOptions: process.env.TP_TEST_BROWSER ? { executablePath: process.env.TP_TEST_BROWSER } : {},
  },
  webServer: { command: "npm run dev -- --host 127.0.0.1 --port 5175", url: "http://127.0.0.1:5175/tests/fixtures/panel.html", reuseExistingServer: true },
});

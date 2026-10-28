import { defineConfig } from "@playwright/test";

const port = Number(process.env.TP_TEST_PORT ?? 5175);
export default defineConfig({
  testDir: "./tests/ui",
  workers: 1,
  use: { baseURL: `http://127.0.0.1:${port}`, viewport: { width: 420, height: 600 }, screenshot: "only-on-failure",
    launchOptions: process.env.TP_TEST_BROWSER ? { executablePath: process.env.TP_TEST_BROWSER } : {},
  },
  webServer: { command: `npm run dev -- --host 127.0.0.1 --port ${port}`, url: `http://127.0.0.1:${port}/tests/fixtures/panel.html`, reuseExistingServer: true },
});

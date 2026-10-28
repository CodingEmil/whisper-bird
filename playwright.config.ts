import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests",
  use: {
    baseURL: "http://127.0.0.1:1422",
    browserName: "chromium",
    channel: process.env.PLAYWRIGHT_CHANNEL,
  },
  webServer: {
    command: "bun run dev --host 127.0.0.1 --port 1422",
    url: "http://127.0.0.1:1422",
    reuseExistingServer: !process.env.CI,
  },
});

import { defineConfig, devices } from '@playwright/test';

const port = 3194;
const localBaseURL = `http://127.0.0.1:${port}`;
const externalBaseURL = process.env.APOCKY_E2E_BASE_URL?.replace(/\/$/, '');
const baseURL = externalBaseURL ?? localBaseURL;

export default defineConfig({
  testDir: './tests/e2e',
  outputDir: './test-results/playwright',
  fullyParallel: true,
  // Next's development server can transiently serve a blank navigation under
  // concurrent compile pressure. Serial execution keeps this local acceptance
  // matrix deterministic; deployment concurrency is tested separately.
  workers: 1,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 2 : 0,
  reporter: [['list'], ['html', { outputFolder: 'test-results/playwright-report', open: 'never' }]],
  use: {
    baseURL,
    colorScheme: 'dark',
    locale: 'en-US',
    contextOptions: { reducedMotion: 'reduce' },
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    video: 'off',
  },
  projects: [
    { name: 'desktop-chrome', grepInvert: /@mobile/, use: { ...devices['Desktop Chrome'], channel: 'chrome', viewport: { width: 1440, height: 900 } } },
    { name: 'mobile-chrome', grepInvert: /@visual/, use: { ...devices['Pixel 5'], channel: 'chrome', viewport: { width: 390, height: 844 } } },
    {
      name: 'ios-webkit-iphone-15-pro',
      grepInvert: /@visual/,
      use: {
        ...devices['iPhone 15 Pro'],
        browserName: 'webkit',
      },
    },
  ],
  webServer: externalBaseURL
    ? undefined
    : {
        command: `npx next dev -p ${port}`,
        url: localBaseURL,
        env: {
          ...process.env,
          NEXT_PUBLIC_SUPABASE_URL: `${localBaseURL}/fake-supabase`,
          NEXT_PUBLIC_SUPABASE_ANON_KEY: 'public-test-key-not-a-secret',
        },
        reuseExistingServer: false,
        timeout: 120_000,
      },
});

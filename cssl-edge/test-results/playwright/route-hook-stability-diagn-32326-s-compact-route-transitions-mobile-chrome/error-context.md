# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: route-hook-stability.spec.ts >> diagnostics consent keeps hook order across compact route transitions
- Location: tests\e2e\route-hook-stability.spec.ts:3:5

# Error details

```
Error: expect(locator).toBeVisible() failed

Locator: getByRole('heading', { name: 'Speak plainly.' })
Expected: visible
Timeout: 5000ms
Error: element(s) not found

Call log:
  - Expect "toBeVisible" with timeout 5000ms
  - waiting for getByRole('heading', { name: 'Speak plainly.' })

```

```yaml
- link "Skip to conversation":
  - /url: "#apocrypha-conversation"
- banner:
  - link "Apocky home":
    - /url: /
  - navigation "Apocrypha navigation":
    - link "The Clearing":
      - /url: /clearing
    - link "Sign in":
      - /url: /login?next=%2Fapocrypha
- main:
  - region "Conversation with Apocrypha":
    - paragraph: APOCRYPHA
    - heading "Intelligence workspace" [level=1]
    - button "New chat"
    - heading "What are we working on?" [level=2]
    - paragraph: Ask, analyze, write, explain, or generate code. Each response is validated and carries an inspectable receipt.
    - term: Chat history
    - definition: This session
    - term: Training consent
    - definition: "Off"
    - term: Effect authority
    - definition: None
    - status:
      - strong: Sign in to begin a restricted member turn.
      - text: No message is sent until the session is verified.
      - link "Sign in":
        - /url: /login?next=%2Fapocrypha
  - complementary "Privacy and response details":
    - group: Privacy & response details
- button "Open Next.js Dev Tools":
  - img
- alert: Speak with Apocrypha · Apocky
```

# Test source

```ts
  1  | import { expect, test } from '@playwright/test';
  2  | 
  3  | test('diagnostics consent keeps hook order across compact route transitions', async ({ page }) => {
  4  |   const browserErrors: string[] = [];
  5  |   page.on('pageerror', (error) => browserErrors.push(error.message));
  6  |   page.on('console', (message) => {
  7  |     if (message.type() === 'error') browserErrors.push(message.text());
  8  |   });
  9  | 
  10 |   await page.route('**/api/auth/me', (route) => route.fulfill({
  11 |     status: 200,
  12 |     contentType: 'application/json',
  13 |     body: JSON.stringify({ user: null }),
  14 |   }));
  15 | 
  16 |   await page.goto('/', { waitUntil: 'domcontentloaded' });
  17 |   await expect(page.getByRole('heading', { name: /relay for minds/i })).toBeVisible();
  18 | 
  19 |   await page.getByRole('link', { name: /open the relay/i }).click();
  20 |   await expect(page).toHaveURL(/\/apocrypha$/);
> 21 |   await expect(page.getByRole('heading', { name: 'Speak plainly.' })).toBeVisible();
     |                                                                       ^ Error: expect(locator).toBeVisible() failed
  22 |   await expect(page.getByRole('heading', { name: 'This page hit an error' })).toHaveCount(0);
  23 | 
  24 |   await page.getByRole('link', { name: 'Apocky home' }).click();
  25 |   await expect(page).toHaveURL(/\/$/);
  26 |   await expect(page.getByRole('heading', { name: /relay for minds/i })).toBeVisible();
  27 |   await expect(page.getByRole('heading', { name: 'This page hit an error' })).toHaveCount(0);
  28 |   expect(browserErrors.filter((message) => /Rendered fewer hooks|Minified React error #300/.test(message))).toEqual([]);
  29 | });
  30 | 
```
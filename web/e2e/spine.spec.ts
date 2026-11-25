import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, test, type Page } from '@playwright/test'

/**
 * The end-to-end spine: the real setup ceremony (a Chromium virtual
 * authenticator stands in for a platform passkey), the dashboard's live
 * stream, navigation, and password sign-in. Nothing is mocked, and the
 * scratch data directory comes from `just e2e`.
 *
 * The agent chat and its transcript are not covered here: they need a live
 * Claude Code session, so the owner proves them by hand.
 */

const PASSWORD = 'dandelion e2e password'
const SERVER_LOG = fileURLToPath(new URL('../../target/e2e/server.log', import.meta.url))

test.describe.configure({ mode: 'serial' })

function setupUrl(): string {
  const printed = readFileSync(SERVER_LOG, 'utf8')
  const match = /Open: (\S+)/.exec(printed)
  if (!match?.[1]) throw new Error(`no setup link in ${SERVER_LOG}`)
  return match[1]
}

/** A user-verifying platform authenticator, so the real WebAuthn flow runs. */
async function addVirtualAuthenticator(page: Page) {
  const session = await page.context().newCDPSession(page)
  await session.send('WebAuthn.enable')
  await session.send('WebAuthn.addVirtualAuthenticator', {
    options: {
      protocol: 'ctap2',
      transport: 'internal',
      hasResidentKey: true,
      hasUserVerification: true,
      isUserVerified: true,
      automaticPresenceSimulation: true,
    },
  })
}

test('setup registers a passkey and a password, and the dashboard goes live', async ({ page }) => {
  await addVirtualAuthenticator(page)
  await page.goto(setupUrl())

  await page.getByRole('button', { name: 'Register a passkey' }).click()
  await expect(page.getByText('Passkey registered.')).toBeVisible()

  await page.getByLabel('At least 12 characters').fill(PASSWORD)
  await page.getByRole('button', { name: 'Set password' }).click()
  await expect(page.getByText('Password set.')).toBeVisible()

  await page.getByRole('button', { name: 'Finish setup' }).click()
  await expect(page).toHaveURL(/\/dashboard$/)
  await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible()

  // Live data: the CPU card shows a number once the first sample lands.
  const cpu = page.getByText('CPU', { exact: true }).locator('..')
  await expect(cpu.locator('span').first()).toHaveText(/\d/)

  await page.getByRole('link', { name: 'Agents' }).click()
  await expect(page.getByRole('heading', { name: 'Agents', level: 1 })).toBeVisible()

  await page.getByRole('link', { name: 'Processes' }).click()
  await expect(page.getByRole('heading', { name: 'Processes', level: 1 })).toBeVisible()
  await expect(page.getByRole('button', { name: 'Refresh' })).toBeVisible()
})

test('the app runs under its policy and keeps a saved theme', async ({ page }) => {
  const violations: string[] = []
  page.on('console', (message) => {
    if (message.type() === 'error') violations.push(message.text())
  })

  const response = await page.goto('/')
  expect(response?.headers()['referrer-policy']).toBe('no-referrer')
  expect(response?.headers()['content-security-policy']).toContain("font-src 'self' data:")

  await page.evaluate(() => localStorage.setItem('dandelion-theme', 'light'))
  await page.reload()
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light')
  expect(violations.filter((text) => text.includes('Content Security Policy'))).toEqual([])
})

test('an unauthenticated visitor signs in with the password', async ({ page }) => {
  await page.goto('/')
  await expect(page).toHaveURL(/\/login$/)

  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()

  await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible()
})

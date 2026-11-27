import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { expect, test, type Locator, type Page } from '@playwright/test'

/**
 * The end-to-end spine: the real setup ceremony (a Chromium virtual
 * authenticator stands in for a platform passkey), the dashboard's live
 * stream, navigation, and password sign-in. Nothing is mocked except the
 * agents snapshot the layout test stubs, and the scratch data directory comes
 * from `just e2e`.
 *
 * The agent card's layout is covered with that stub; its transcript content
 * still needs a live Claude Code session, so the owner proves that by hand.
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

  // The dashboard's Agents panel links here too ("Open agents"), so the name
  // must match the nav link exactly.
  await page.getByRole('link', { name: 'Agents', exact: true }).click()
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

/** Answers `/api/agents` with one synthetic agent, so no backend is needed. */
async function stubAgents(page: Page, blocked: 'question' | null) {
  await page.route('**/api/agents', async (route) => {
    if (route.request().method() !== 'GET') return route.continue()
    const response = await route.fetch()
    const body = (await response.json()) as Record<string, unknown>
    body.available = true
    body.backend = { kind: 'herdr-core', detail: null }
    body.agents = [
      {
        id: 'stub1',
        title: 'stub agent',
        cwd: '/tmp',
        labels: {},
        agent_kind: 'claude-code',
        agent_state: {
          state: blocked ? 'blocked' : 'working',
          blocked_reason: blocked,
          progress: null,
        },
        lifecycle: 'live',
        created_ms: 1_700_000_000_000,
        exit_code: null,
        finished_reason: null,
      },
    ]
    body.claude = [
      {
        terminal: 'stub1',
        sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
        cwd: '/tmp',
        name: 'stub',
        agent: null,
        version: '2.1.330',
        status: 'waiting',
        waitingFor: 'input needed',
        updatedAt: 1_795_372_085_544,
        usage: {
          sessionId: '0f3c2b1a-aaaa-bbbb-cccc-0123456789ab',
          models: [],
          tokens: {
            inputTokens: 96,
            outputTokens: 39_430,
            cacheReadInputTokens: 3_233_619,
            cacheCreationInputTokens: 129_107,
          },
          costUsd: 0.42,
          unpricedModels: [],
          messages: 41,
        },
      },
    ]
    await route.fulfill({ response, json: body })
  })
}

/** The card's bottom must never sit above its composer's bottom. */
async function expectInside(composer: Locator) {
  const cardBox = await composer.locator('xpath=ancestor::section[1]').boundingBox()
  const composerBox = await composer.boundingBox()
  if (!cardBox || !composerBox) throw new Error('the card or the composer has no box')
  expect(composerBox.y + composerBox.height).toBeLessThanOrEqual(cardBox.y + cardBox.height + 1)
}

test('the agent card holds its composer at every size', async ({ page }) => {
  await page.goto('/')
  await expect(page).toHaveURL(/\/login$/)
  await page.getByLabel('Password').fill(PASSWORD)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible()

  const sizes = [
    { width: 1280, height: 900 },
    { width: 844, height: 390 },
    { width: 768, height: 600 },
    { width: 390, height: 844 },
  ]

  // Reduced motion makes the selection scroll jump instead of animate, so the
  // measurements below never race a running animation.
  await page.emulateMedia({ reducedMotion: 'reduce' })
  const card = page
    .locator('section')
    .filter({ has: page.getByRole('button', { name: 'Full screen' }) })

  // Picking the row is what brings the card to the top of the page.
  const select = async () => {
    await page.goto('/agents')
    await page.getByRole('button', { name: 'stub agent' }).click()
    // The session's meta row is what makes the card tall enough for this test
    // to mean anything, so it has to be there before anything is measured.
    await expect(page.getByText('Claude session')).toBeVisible()
    await expect.poll(async () => (await card.boundingBox())?.y ?? -1).toBeLessThanOrEqual(70)
  }

  // A question: the answer bar is the tallest thing the card holds.
  await stubAgents(page, 'question')
  for (const size of sizes) {
    await page.setViewportSize(size)
    await select()
    await expect(page.getByRole('form', { name: 'Answer the question' })).toBeVisible()
    await expectInside(page.getByRole('form', { name: 'Answer the question' }))
  }

  // A working agent: the prompt form, which must also sit inside the card and
  // — on a full screen — inside the viewport.
  await stubAgents(page, null)
  for (const size of sizes) {
    await page.setViewportSize(size)
    await select()
    const send = page.getByRole('button', { name: 'Send' })
    await expect(send).toBeVisible()
    await expectInside(send.locator('xpath=ancestor::form[1]'))
    if (size.height >= 900) {
      const box = await send.boundingBox()
      if (!box) throw new Error('Send has no box')
      expect(box.y + box.height).toBeLessThanOrEqual(size.height)
    }
  }
})

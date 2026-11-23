import {
  browserSupportsWebAuthn,
  startAuthentication,
  startRegistration,
  type AuthenticationResponseJSON,
  type PublicKeyCredentialCreationOptionsJSON,
  type PublicKeyCredentialRequestOptionsJSON,
  type RegistrationResponseJSON,
} from '@simplewebauthn/browser'

export interface AuthStatus {
  setup_required: boolean
  authenticated: boolean
}

export interface SetupStatus {
  valid: boolean
  has_password: boolean
  has_passkey: boolean
}

/** The server sends `{ ceremony_id, options: { publicKey: … } }`. */
interface Challenge<TOptions> {
  ceremony_id: string
  options: { publicKey: TOptions }
}

export class AuthRequestError extends Error {
  readonly status: number
  readonly code: string

  constructor(status: number, code: string, message: string) {
    super(message)
    this.name = 'AuthRequestError'
    this.status = status
    this.code = code
  }
}

function describeError(data: unknown, status: number): { code: string; message: string } {
  if (typeof data === 'object' && data !== null) {
    const record = data as Record<string, unknown>
    return {
      code: typeof record.error === 'string' ? record.error : 'request_failed',
      message: typeof record.message === 'string' ? record.message : `Request failed (${status})`,
    }
  }
  return { code: 'request_failed', message: `Request failed (${status})` }
}

async function request<T>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, {
    method,
    credentials: 'same-origin',
    headers: body === undefined ? undefined : { 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const data: unknown = await response.json().catch(() => null)
  if (!response.ok) {
    const { code, message } = describeError(data, response.status)
    throw new AuthRequestError(response.status, code, message)
  }
  // The server's generated contract types are opaque for WebAuthn payloads;
  // each caller names the shape it expects.
  return data as T
}

export function passkeysSupported(): boolean {
  try {
    return browserSupportsWebAuthn()
  } catch {
    return false
  }
}

export function fetchAuthStatus(): Promise<AuthStatus> {
  return request('GET', '/api/auth/status')
}

export async function loginWithPassword(password: string): Promise<void> {
  await request('POST', '/api/auth/login/password', { password })
}

export async function loginWithPasskey(): Promise<void> {
  const start = await request<Challenge<PublicKeyCredentialRequestOptionsJSON>>(
    'POST',
    '/api/auth/login/passkey/start',
  )
  const credential: AuthenticationResponseJSON = await startAuthentication({
    optionsJSON: start.options.publicKey,
  })
  await request('POST', '/api/auth/login/passkey/finish', {
    ceremony_id: start.ceremony_id,
    credential,
  })
}

export function fetchSetupStatus(token: string): Promise<SetupStatus> {
  return request('GET', `/api/auth/setup/status?token=${encodeURIComponent(token)}`)
}

export async function registerSetupPasskey(token: string): Promise<SetupStatus> {
  const start = await request<Challenge<PublicKeyCredentialCreationOptionsJSON>>(
    'POST',
    '/api/auth/setup/passkey/start',
    { token },
  )
  const credential: RegistrationResponseJSON = await startRegistration({
    optionsJSON: start.options.publicKey,
  })
  return request('POST', '/api/auth/setup/passkey/finish', {
    token,
    ceremony_id: start.ceremony_id,
    credential,
  })
}

export function setSetupPassword(token: string, password: string): Promise<SetupStatus> {
  return request('POST', '/api/auth/setup/password', { token, password })
}

export async function completeSetup(token: string): Promise<void> {
  await request('POST', '/api/auth/setup/complete', { token })
}

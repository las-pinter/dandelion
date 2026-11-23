import { Check, Fingerprint } from 'lucide-react'
import { type FormEvent, useEffect, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  completeSetup,
  fetchSetupStatus,
  passkeysSupported,
  registerSetupPasskey,
  setSetupPassword,
  type SetupStatus,
} from '@/lib/auth'

const MIN_PASSWORD_CHARS = 12

export interface SetupFormProps {
  /** The single-use token from the setup link. */
  token: string
  /** Called after the setup token is consumed; defaults to a reload of `/`. */
  onComplete?: () => void
  /** Injectable for tests and stories; defaults to the real endpoint. */
  loadStatus?: (token: string) => Promise<SetupStatus>
}

type Phase = 'loading' | 'invalid' | 'error' | 'ready'
type Pending = 'none' | 'passkey' | 'password' | 'finish'

/** Registers a passkey and a password for a fresh owner. */
export function SetupForm({ token, onComplete, loadStatus = fetchSetupStatus }: SetupFormProps) {
  const [phase, setPhase] = useState<Phase>('loading')
  const [status, setStatus] = useState<SetupStatus>({
    valid: false,
    has_password: false,
    has_passkey: false,
  })
  const [loadError, setLoadError] = useState<string | null>(null)
  const [password, setPassword] = useState('')
  const [pending, setPending] = useState<Pending>('none')
  const [error, setError] = useState<string | null>(null)
  const [supportsPasskeys] = useState(passkeysSupported)

  useEffect(() => {
    let active = true
    loadStatus(token)
      .then((next) => {
        if (!active) return
        setStatus(next)
        setPhase(next.valid ? 'ready' : 'invalid')
      })
      .catch((cause: unknown) => {
        if (!active) return
        setLoadError(cause instanceof Error ? cause.message : 'Cannot reach the server.')
        setPhase('error')
      })
    return () => {
      active = false
    }
  }, [token, loadStatus])

  function succeed() {
    if (onComplete) {
      onComplete()
      return
    }
    window.location.assign('/')
  }

  async function run(kind: Exclude<Pending, 'none'>, action: () => Promise<void>) {
    setError(null)
    setPending(kind)
    try {
      await action()
      setPending('none')
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Setup failed.')
      setPending('none')
    }
  }

  function onPasswordSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (password.length < MIN_PASSWORD_CHARS) {
      setError(`Use at least ${MIN_PASSWORD_CHARS} characters.`)
      return
    }
    void run('password', async () => {
      setStatus(await setSetupPassword(token, password))
    })
  }

  if (phase === 'loading') {
    return (
      <p role="status" className="text-sm text-muted-foreground">
        Checking the setup link…
      </p>
    )
  }

  if (phase === 'invalid') {
    return (
      <div className="flex flex-col gap-2 text-sm">
        <p className="font-medium">This setup link is invalid or has expired.</p>
        <p className="text-muted-foreground">
          Run{' '}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
            dandelion auth reset
          </code>{' '}
          on the server to print a new one.
        </p>
      </div>
    )
  }

  if (phase === 'error') {
    return (
      <div className="flex flex-col gap-3 text-sm">
        <p role="alert">{loadError ?? 'Cannot reach the server.'}</p>
        <Button type="button" variant="outline" onClick={() => window.location.reload()}>
          Try again
        </Button>
      </div>
    )
  }

  const complete = status.has_password && status.has_passkey

  return (
    <div className="flex flex-col gap-5">
      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">Passkey</h2>
        {status.has_passkey ? (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Check aria-hidden className="size-4 text-primary" />
            Passkey registered.
          </p>
        ) : (
          <Button
            type="button"
            size="lg"
            className="w-full"
            disabled={pending !== 'none' || !supportsPasskeys}
            onClick={() =>
              void run('passkey', async () => {
                setStatus(await registerSetupPasskey(token))
              })
            }
          >
            <Fingerprint aria-hidden />
            {pending === 'passkey' ? 'Waiting for the passkey…' : 'Register a passkey'}
          </Button>
        )}
        {!status.has_passkey && !supportsPasskeys && (
          <p className="text-xs text-muted-foreground">
            This browser does not offer passkeys; try a browser with platform authenticators.
          </p>
        )}
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium">Password</h2>
        {status.has_password ? (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Check aria-hidden className="size-4 text-primary" />
            Password set.
          </p>
        ) : (
          <form onSubmit={onPasswordSubmit} className="flex flex-col gap-2">
            <label htmlFor="setup-password" className="text-sm text-muted-foreground">
              At least {MIN_PASSWORD_CHARS} characters
            </label>
            <Input
              id="setup-password"
              type="password"
              autoComplete="new-password"
              minLength={MIN_PASSWORD_CHARS}
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              disabled={pending !== 'none'}
            />
            <Button
              type="submit"
              variant="outline"
              size="lg"
              className="w-full"
              disabled={pending !== 'none'}
            >
              {pending === 'password' ? 'Saving…' : 'Set password'}
            </Button>
          </form>
        )}
      </section>

      {error !== null && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}

      <Button
        type="button"
        size="lg"
        className="w-full"
        disabled={!complete || pending !== 'none'}
        onClick={() =>
          void run('finish', async () => {
            await completeSetup(token)
            succeed()
          })
        }
      >
        {pending === 'finish' ? 'Finishing…' : 'Finish setup'}
      </Button>
      {!complete && (
        <p className="text-xs text-muted-foreground">
          Register a passkey and set a password to finish.
        </p>
      )}
    </div>
  )
}

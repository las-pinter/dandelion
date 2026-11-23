import { Fingerprint } from 'lucide-react'
import { type FormEvent, useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { loginWithPasskey, loginWithPassword, passkeysSupported } from '@/lib/auth'

export interface LoginFormProps {
  /** Called after the cookie is set; defaults to a full reload of `/`. */
  onSuccess?: () => void
}

type Pending = 'none' | 'passkey' | 'password'

/** Sign-in with a passkey or the owner password. */
export function LoginForm({ onSuccess }: LoginFormProps = {}) {
  const [password, setPassword] = useState('')
  const [pending, setPending] = useState<Pending>('none')
  const [error, setError] = useState<string | null>(null)
  const [supportsPasskeys] = useState(passkeysSupported)

  function succeed() {
    if (onSuccess) {
      onSuccess()
      return
    }
    window.location.assign('/')
  }

  async function run(kind: Exclude<Pending, 'none'>, action: () => Promise<void>) {
    setError(null)
    setPending(kind)
    try {
      await action()
      succeed()
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Sign-in failed.')
      setPending('none')
    }
  }

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (password.length === 0) {
      setError('Enter the password.')
      return
    }
    void run('password', () => loginWithPassword(password))
  }

  return (
    <div className="flex flex-col gap-4">
      {supportsPasskeys && (
        <>
          <Button
            type="button"
            size="lg"
            className="w-full"
            disabled={pending !== 'none'}
            onClick={() => void run('passkey', loginWithPasskey)}
          >
            <Fingerprint aria-hidden />
            {pending === 'passkey' ? 'Waiting for the passkey…' : 'Use a passkey'}
          </Button>
          <div className="flex items-center gap-3 text-xs text-muted-foreground">
            <span className="h-px flex-1 bg-border" />
            or use the password
            <span className="h-px flex-1 bg-border" />
          </div>
        </>
      )}

      <form onSubmit={onSubmit} className="flex flex-col gap-3">
        <div className="flex flex-col gap-1.5">
          <label htmlFor="login-password" className="text-sm font-medium">
            Password
          </label>
          <Input
            id="login-password"
            type="password"
            autoComplete="current-password"
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            disabled={pending !== 'none'}
          />
        </div>
        <Button
          type="submit"
          variant={supportsPasskeys ? 'outline' : 'default'}
          size="lg"
          className="w-full"
          disabled={pending !== 'none'}
        >
          {pending === 'password' ? 'Signing in…' : 'Sign in'}
        </Button>
      </form>

      {!supportsPasskeys && (
        <p className="text-xs text-muted-foreground">
          This browser does not offer passkeys; the owner password still works.
        </p>
      )}
      {error !== null && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}

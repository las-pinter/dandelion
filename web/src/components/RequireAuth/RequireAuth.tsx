import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react'
import { Navigate } from 'react-router'
import { Button } from '@/components/ui/button'
import { fetchAuthStatus, type AuthStatus } from '@/lib/auth'

export interface RequireAuthProps {
  children: ReactNode
  /** Injectable for tests and stories; defaults to the real endpoint. */
  loadStatus?: () => Promise<AuthStatus>
}

type State =
  | { phase: 'loading' }
  | { phase: 'error'; message: string }
  | { phase: 'ready'; status: AuthStatus }

/** Sends a browser without a session to sign in, or to setup when none exists. */
export function RequireAuth({ children, loadStatus = fetchAuthStatus }: RequireAuthProps) {
  const [state, setState] = useState<State>({ phase: 'loading' })
  const sequence = useRef(0)
  const mounted = useRef(true)

  const refresh = useCallback(() => {
    const request = sequence.current + 1
    sequence.current = request
    loadStatus()
      .then((status) => {
        if (mounted.current && sequence.current === request) setState({ phase: 'ready', status })
      })
      .catch((cause: unknown) => {
        if (!mounted.current || sequence.current !== request) return
        setState({
          phase: 'error',
          message: cause instanceof Error ? cause.message : 'Cannot reach the server.',
        })
      })
  }, [loadStatus])

  useEffect(() => {
    mounted.current = true
    refresh()
    return () => {
      mounted.current = false
    }
  }, [refresh])

  function retry() {
    setState({ phase: 'loading' })
    refresh()
  }

  if (state.phase === 'loading') {
    return (
      <div className="flex min-h-svh items-center justify-center bg-background text-sm text-muted-foreground">
        <p role="status">Loading Dandelion…</p>
      </div>
    )
  }

  if (state.phase === 'error') {
    return (
      <div className="flex min-h-svh flex-col items-center justify-center gap-3 bg-background p-4 text-center">
        <p role="alert" className="text-sm text-destructive">
          {state.message}
        </p>
        <Button type="button" variant="outline" onClick={retry}>
          Try again
        </Button>
      </div>
    )
  }

  if (state.status.setup_required) {
    return <Navigate to="/setup" replace />
  }
  if (!state.status.authenticated) {
    return <Navigate to="/login" replace />
  }

  return <>{children}</>
}

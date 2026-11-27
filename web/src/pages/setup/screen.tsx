import { useSearchParams } from 'react-router'
import { AuthLayout } from '@/components/AuthLayout/AuthLayout'
import { SetupForm } from '@/components/SetupForm/SetupForm'

export function SetupPage() {
  const [params] = useSearchParams()
  const token = params.get('token') ?? ''

  return (
    <AuthLayout
      title="Set up Dandelion"
      description="Register a passkey and set the owner password."
    >
      {token === '' ? (
        <p className="text-sm text-muted-foreground">
          Open the setup link printed by the server, or run{' '}
          <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">
            dandelion auth reset
          </code>{' '}
          for a new one.
        </p>
      ) : (
        <SetupForm token={token} />
      )}
    </AuthLayout>
  )
}

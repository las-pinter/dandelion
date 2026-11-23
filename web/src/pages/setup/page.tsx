import { KeyRound } from 'lucide-react'
import { useSearchParams } from 'react-router'
import type { PageDefinition } from '@/app/page-registry'
import { AuthLayout } from '@/components/AuthLayout/AuthLayout'
import { SetupForm } from '@/components/SetupForm/SetupForm'

function SetupPage() {
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

export const page: PageDefinition = {
  path: '/setup',
  title: 'Setup',
  icon: KeyRound,
  order: 901,
  standalone: true,
  Component: SetupPage,
}

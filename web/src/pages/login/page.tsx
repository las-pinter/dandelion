import { LogIn } from 'lucide-react'
import type { PageDefinition } from '@/app/page-registry'
import { AuthLayout } from '@/components/AuthLayout/AuthLayout'
import { LoginForm } from '@/components/LoginForm/LoginForm'

function LoginPage() {
  return (
    <AuthLayout title="Sign in" description="Use a passkey or the owner password.">
      <LoginForm />
    </AuthLayout>
  )
}

export const page: PageDefinition = {
  path: '/login',
  title: 'Sign in',
  icon: LogIn,
  order: 900,
  standalone: true,
  Component: LoginPage,
}

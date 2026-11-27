import { AuthLayout } from '@/components/AuthLayout/AuthLayout'
import { LoginForm } from '@/components/LoginForm/LoginForm'

export function LoginPage() {
  return (
    <AuthLayout title="Sign in" description="Use a passkey or the owner password.">
      <LoginForm />
    </AuthLayout>
  )
}

import { Link } from 'react-router'

export function NotFound() {
  return (
    <section className="flex flex-col items-start gap-2">
      <h1 className="font-heading text-2xl font-semibold">Page not found</h1>
      <p className="text-muted-foreground">Nothing lives at this address.</p>
      <Link to="/" className="text-sm underline underline-offset-4">
        Back to the start
      </Link>
    </section>
  )
}

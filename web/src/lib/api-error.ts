/**
 * The API error message from an error body, or a fallback when the failure
 * carries none.
 */
export function messageOf(failure: unknown, fallback = 'the request failed'): string {
  if (
    typeof failure === 'object' &&
    failure !== null &&
    'message' in failure &&
    typeof failure.message === 'string'
  ) {
    return failure.message
  }
  return fallback
}

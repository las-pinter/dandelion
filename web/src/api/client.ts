import createClient from 'openapi-fetch'
import type { paths } from './gen/schema'

/** Typed client for the Dandelion HTTP API. */
export const api = createClient<paths>({ baseUrl: '/' })

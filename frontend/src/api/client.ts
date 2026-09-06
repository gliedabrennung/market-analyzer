import type { ApiErrorBody } from './types'

const ARROW_MIME = 'application/vnd.apache.arrow.stream'

/** The FR-5.2 error envelope, thrown as a real `Error` so panels can catch
 * it, show `message`, and use `code`/`details` for anything more specific
 * (FR-1.5: one panel's error must not crash the rest of the UI). */
export class ApiRequestError extends Error {
  readonly status: number
  readonly code: string
  readonly details: unknown

  constructor(status: number, body: ApiErrorBody) {
    super(body.error.message)
    this.name = 'ApiRequestError'
    this.status = status
    this.code = body.error.code
    this.details = body.error.details
  }
}

export interface FetchRowsOptions {
  signal?: AbortSignal
}

/** Fetches `path`, preferring Arrow IPC (frontend-tz.md FR-1.1) and
 * transparently falling back to JSON (FR-1.2) when the server can't or
 * won't produce Arrow — either a `406`, or a `200` with a non-Arrow
 * `Content-Type`. `parseArrow` turns IPC bytes into rows of `T`; the JSON
 * body is assumed to already be an array of that same row shape.
 */
export async function fetchRows<T>(
  path: string,
  parseArrow: (bytes: Uint8Array) => T[],
  options: FetchRowsOptions = {},
): Promise<T[]> {
  let res = await fetch(path, {
    headers: { Accept: ARROW_MIME },
    signal: options.signal,
  })

  if (res.status === 406) {
    logJsonFallback(path, 'server returned 406 for Arrow')
    res = await fetch(path, {
      headers: { Accept: 'application/json' },
      signal: options.signal,
    })
  }

  if (!res.ok) {
    throw await toApiError(res)
  }

  const contentType = res.headers.get('content-type') ?? ''
  if (contentType.includes(ARROW_MIME)) {
    return parseArrow(new Uint8Array(await res.arrayBuffer()))
  }

  logJsonFallback(path, `content-type was '${contentType}'`)
  return (await res.json()) as T[]
}

function logJsonFallback(path: string, reason: string): void {
  if (import.meta.env.DEV) {
    console.warn(`[api] JSON fallback for ${path}: ${reason}`)
  }
}

async function toApiError(res: Response): Promise<ApiRequestError> {
  try {
    const body = (await res.json()) as ApiErrorBody
    return new ApiRequestError(res.status, body)
  } catch {
    return new ApiRequestError(res.status, {
      error: {
        code: 'unknown_error',
        message: res.statusText || `HTTP ${res.status}`,
        details: {},
      },
    })
  }
}

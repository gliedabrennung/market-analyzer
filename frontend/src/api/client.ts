import type { ApiErrorBody } from './types'

const ARROW_MIME = 'application/vnd.apache.arrow.stream'

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

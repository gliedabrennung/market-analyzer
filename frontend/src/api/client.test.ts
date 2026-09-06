import { afterEach, describe, expect, it, vi } from 'vitest'
import { ApiRequestError, fetchRows } from './client'

const ARROW_MIME = 'application/vnd.apache.arrow.stream'

function jsonResponse(body: unknown, init: ResponseInit = {}): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
    ...init,
  })
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('fetchRows', () => {
  it('parses Arrow bytes when the server returns the Arrow content-type', async () => {
    const buffer = new Uint8Array([1, 2, 3]).buffer
    vi.stubGlobal(
      'fetch',
      vi
        .fn()
        .mockResolvedValue(new Response(buffer, { status: 200, headers: { 'content-type': ARROW_MIME } })),
    )
    const parseArrow = vi.fn().mockReturnValue([{ ok: true }])

    const rows = await fetchRows('/x', parseArrow)

    expect(parseArrow).toHaveBeenCalledTimes(1)
    expect(rows).toEqual([{ ok: true }])
  })

  it('falls back to JSON when the response content-type is not Arrow (FR-1.2)', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse([{ a: 1 }])))
    const parseArrow = vi.fn()

    const rows = await fetchRows('/x', parseArrow)

    expect(parseArrow).not.toHaveBeenCalled()
    expect(rows).toEqual([{ a: 1 }])
  })

  it('retries with an explicit JSON Accept header on 406 (FR-1.2)', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(new Response(null, { status: 406 }))
      .mockResolvedValueOnce(jsonResponse([{ a: 2 }]))
    vi.stubGlobal('fetch', fetchMock)

    const rows = await fetchRows('/x', vi.fn())

    expect(fetchMock).toHaveBeenCalledTimes(2)
    expect(fetchMock.mock.calls[1]?.[1]?.headers).toMatchObject({ Accept: 'application/json' })
    expect(rows).toEqual([{ a: 2 }])
  })

  it('throws an ApiRequestError carrying the FR-5.2 envelope fields on a non-ok response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(
        jsonResponse(
          {
            error: { code: 'unknown_symbol', message: "unknown symbol 'FOO'", details: { symbol: 'FOO' } },
          },
          { status: 404 },
        ),
      ),
    )

    await expect(fetchRows('/x', vi.fn())).rejects.toMatchObject({
      status: 404,
      code: 'unknown_symbol',
      message: "unknown symbol 'FOO'",
    } satisfies Partial<ApiRequestError>)
  })
})

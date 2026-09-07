import { describe, expect, it } from 'vitest'
import { parseLiveMessage } from './socket'

// Captured live from the real backend during development (`ws://.../stream/BTCUSDT?interval=1m`)
// — not hand-written fixtures, the actual `ma_core::MarketEvent` serde
// output (frontend-tz.md BE-5).
const REAL_TRADE_FRAME =
  '{"type":"trade","ts":"2026-09-07T07:58:56.971Z","symbol":"BTCUSDT","exchange":"binance","trade_id":6660754743,"price":"79400.00000000","qty":"0.00037000","is_buyer_maker":false}'

const REAL_KLINE_FRAME =
  '{"type":"kline","open_time":"2026-09-07T07:58:00Z","close_time":"2026-09-07T07:58:59.999Z","symbol":"BTCUSDT","exchange":"binance","interval":"1m","open":"79396.68000000","high":"79400.00000000","low":"79388.40000000","close":"79399.99000000","volume":"5.78367000","quote_volume":"459174.00334350","trades_count":842,"is_closed":false}'

const REAL_HEARTBEAT_FRAME = '{"type":"heartbeat"}'

describe('parseLiveMessage', () => {
  it('parses a real trade frame', () => {
    const parsed = parseLiveMessage(REAL_TRADE_FRAME)
    expect(parsed).toEqual({
      kind: 'trade',
      symbol: 'BTCUSDT',
      trade: {
        ts: Date.parse('2026-09-07T07:58:56.971Z'),
        tradeId: 6660754743,
        price: 79400,
        qty: 0.00037,
        isBuyerMaker: false,
      },
    })
  })

  it('parses a real kline frame, mapping straight to OhlcvRow', () => {
    const parsed = parseLiveMessage(REAL_KLINE_FRAME)
    expect(parsed).toEqual({
      kind: 'kline',
      symbol: 'BTCUSDT',
      interval: '1m',
      row: {
        openTime: Date.parse('2026-09-07T07:58:00Z'),
        closeTime: Date.parse('2026-09-07T07:58:59.999Z'),
        open: 79396.68,
        high: 79400,
        low: 79388.4,
        close: 79399.99,
        volume: 5.78367,
        quoteVolume: 459174.0033435,
        tradesCount: 842,
        isClosed: false,
      },
    })
  })

  it('parses a heartbeat frame', () => {
    expect(parseLiveMessage(REAL_HEARTBEAT_FRAME)).toEqual({ kind: 'heartbeat' })
  })

  it('returns null for malformed JSON rather than throwing', () => {
    expect(parseLiveMessage('not json')).toBeNull()
  })

  it('returns null for an unrecognized type rather than throwing', () => {
    expect(parseLiveMessage('{"type":"something_future"}')).toBeNull()
  })

  // Lightweight Charts throws on a non-finite value ("Assertion failed:
  // ... value=NaN"), and it throws from inside the effect that applies the
  // tick — taking the whole chart panel down until the page is reloaded.
  // A frame that cannot produce real numbers must be dropped here instead.
  it('drops a frame with an unparseable number instead of yielding NaN', () => {
    const broken = REAL_KLINE_FRAME.replace('"close":"79399.99000000"', '"close":"n/a"')
    expect(parseLiveMessage(broken)).toBeNull()
  })

  it('drops a frame missing a numeric field entirely', () => {
    const broken = REAL_TRADE_FRAME.replace('"price":"79400.00000000",', '')
    expect(parseLiveMessage(broken)).toBeNull()
  })

  // The consumer filters by these before applying anything to the chart:
  // a frame from the pair being switched away from must be identifiable.
  it('reports which symbol and interval a frame belongs to', () => {
    const parsed = parseLiveMessage(REAL_KLINE_FRAME)
    expect(parsed).toMatchObject({ symbol: 'BTCUSDT', interval: '1m' })
  })
})

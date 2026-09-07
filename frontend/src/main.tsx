/* @refresh reload */
import { render } from 'solid-js/web'
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query'
import { App } from './app'
import './styles/main.css'

// NFR-1.8: switching back to an interval whose data is already cached
// must not re-hit the network. TanStack Query's own default (`staleTime:
// 0`) treats every cache entry as stale the instant it lands, so it would
// still fire a background refetch on every re-visit even though the
// render itself comes from cache instantly — technically failing the
// letter of NFR-1.8 ("без сетевого запроса"), not just its spirit. 30s is
// long enough to cover a user flipping between a couple of intervals to
// compare them (the actual scenario the requirement describes) while
// still refreshing well within a session if the tab stays open — and the
// live WS path (Этап 3) is already what keeps the *actively viewed*
// interval's last bar current in real time regardless of this value.
const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000 } } })

const root = document.getElementById('root')
if (root === null) {
  throw new Error("index.html is missing the '#root' element")
}

render(
  () => (
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  ),
  root,
)

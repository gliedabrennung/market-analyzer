/* @refresh reload */
import { render } from 'solid-js/web'
import { QueryClient, QueryClientProvider } from '@tanstack/solid-query'
import { App } from './app'
import './styles/main.css'

const queryClient = new QueryClient()

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

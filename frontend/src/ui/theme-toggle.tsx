import Moon from 'lucide-solid/icons/moon'
import Sun from 'lucide-solid/icons/sun'
import { colorblindPalette, setColorblindPalette, theme, toggleTheme } from '../state/theme'
import { ToggleCheckbox } from './toggle-checkbox'

export function ThemeControls() {
  return (
    <div class="flex items-center gap-3 border-l border-[var(--color-border)] pl-4">
      <button
        type="button"
        onClick={toggleTheme}
        title={theme() === 'dark' ? 'Светлая тема' : 'Тёмная тема'}
        aria-label={theme() === 'dark' ? 'Включить светлую тему' : 'Включить тёмную тему'}
        class="flex h-6 w-6 items-center justify-center rounded-md text-[var(--color-fg-muted)] hover:bg-[var(--color-surface-2)] hover:text-[var(--color-fg)]"
      >
        {theme() === 'dark' ? <Sun size={15} /> : <Moon size={15} />}
      </button>
      <ToggleCheckbox label="Дальтоник" checked={colorblindPalette()} onChange={setColorblindPalette} />
    </div>
  )
}

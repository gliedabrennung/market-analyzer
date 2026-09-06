export interface ToggleCheckboxProps {
  label: string
  checked: boolean
  onChange: (checked: boolean) => void
}

export function ToggleCheckbox(props: ToggleCheckboxProps) {
  return (
    <label class="flex cursor-pointer items-center gap-1.5 text-sm text-[var(--color-fg-muted)] select-none">
      <input
        type="checkbox"
        checked={props.checked}
        onChange={(e) => props.onChange(e.currentTarget.checked)}
        class="accent-[var(--color-accent)]"
      />
      {props.label}
    </label>
  )
}

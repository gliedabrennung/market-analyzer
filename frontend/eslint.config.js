import js from '@eslint/js'
import solid from 'eslint-plugin-solid'
import tseslint from 'typescript-eslint'
import globals from 'globals'

export default tseslint.config(
  { ignores: ['dist/**', 'playwright-report/**', 'test-results/**'] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['**/*.{ts,tsx}'],
    plugins: { solid },
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
    },
    rules: {
      ...solid.configs.recommended.rules,
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      'no-empty': ['error', { allowEmptyCatch: true }],
    },
  },
  {
    files: ['scripts/**/*.mjs', '*.config.ts', '*.config.js'],
    languageOptions: {
      globals: globals.node,
    },
  },
)

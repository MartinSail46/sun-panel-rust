import antfu from '@antfu/eslint-config'

export default antfu({
  vue: true,
  typescript: true,
  // Keep this dependency upgrade focused on behavior instead of reformatting legacy declarations.
  stylistic: false,
  jsonc: false,
  yaml: false,
  toml: false,
  markdown: false,
  ignores: ['service-rust/**', 'data/**', 'dist/**'],
  rules: {
    'no-console': 'off',
    'perfectionist/sort-imports': 'off',
    'perfectionist/sort-named-imports': 'off',
    'perfectionist/sort-named-exports': 'off',
    'perfectionist/sort-exports': 'off',
    'antfu/top-level-function': 'off',
  },
}, {
  files: ['tests/**'],
  rules: { 'test/no-import-node-test': 'off' },
})

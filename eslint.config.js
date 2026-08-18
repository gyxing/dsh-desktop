import eslint from '@eslint/js';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist/**', 'src-tauri/**', 'src-tauri/resources/**'] },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ['scripts/**/*.mjs'],
    languageOptions: {
      globals: {
        console: 'readonly',
        fetch: 'readonly',
        process: 'readonly',
        URL: 'readonly',
      },
    },
  },
  {
    files: ['src/**/*.ts', 'vite.config.ts', 'scripts/**/*.mjs'],
    rules: {
      '@typescript-eslint/consistent-type-imports': 'error',
    },
  },
);

import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';

const nodeMajorVersion = Number.parseInt(process.versions.node.split('.')[0], 10);
if (nodeMajorVersion >= 25) {
  const nodeOptions = process.env.NODE_OPTIONS ?? '';
  if (!nodeOptions.includes('--no-experimental-webstorage')) {
    process.env.NODE_OPTIONS = `${nodeOptions} --no-experimental-webstorage`.trim();
  }
}

export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/__tests__/setup.ts'],
    exclude: ['**/node_modules/**', '**/dist/**', 'tests/e2e/**', 'tests/native-smoke/**'],
    coverage: {
      reporter: ['text', 'json', 'html'],
      exclude: ['**/node_modules/**', '**/*.test.tsx?', '**/test/**'],
    },
  },
});

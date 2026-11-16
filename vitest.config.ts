import { defineConfig } from 'vitest/config'

// D2：前端单测 + 覆盖率观察。
// 覆盖率门槛（核心逻辑层 core/adapters/commands ≥70% / 全量 TS ≥40%）在 verify.sh 以 WARN 形式判定，
// 不放进 coverage.thresholds —— 观察期内不因覆盖率未达标而 FAIL。
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.spec.ts'],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json-summary'],
      include: ['src/**/*.ts'],
      exclude: ['src/**/__tests__/**', 'src/main.ts', 'src/vite-env.d.ts', 'src/shared/types.ts'],
    },
  },
})

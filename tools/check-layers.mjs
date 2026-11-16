// D1 分层依赖检查：反向引用与非法跨层引用构建期可检出（npm run build 前置）。
// 层级（低 → 高）：shared → events/adapters → core → commands → components → views → entry(src 根)
// 规则：只允许向上依赖；同层内互引放行；无法归层的 src 内文件按违规处理，防止绕过。
import { readdirSync, readFileSync, existsSync, statSync } from 'node:fs'
import { join, dirname, resolve, relative, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const srcDir = join(root, 'src')

const LAYERS = {
  shared: [],
  events: ['shared'],
  adapters: ['shared'],
  core: ['shared', 'events', 'adapters', 'core'],
  commands: ['shared', 'events', 'adapters', 'core', 'commands'],
  components: ['shared', 'events', 'adapters', 'core', 'commands', 'components'],
  views: ['shared', 'events', 'adapters', 'core', 'commands', 'components', 'views'],
  entry: ['shared', 'events', 'adapters', 'core', 'commands', 'components', 'views', 'entry'],
}

function classify(relPath) {
  const p = relPath.split(sep).join('/')
  for (const layer of ['shared', 'events', 'adapters', 'core', 'commands', 'components', 'views']) {
    if (p.startsWith(layer + '/')) return layer
  }
  return 'entry' // src 根部文件：main.ts / monaco-setup.ts / vite-env.d.ts
}

function walk(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) out.push(...walk(full))
    else if (/\.(ts|vue)$/.test(name)) out.push(full)
  }
  return out
}

// from '...' / import '...' / import('...') / vi.mock('...')
const SPEC_RE = [
  /\bfrom\s*['"]([^'"]+)['"]/g,
  /\bimport\s*['"]([^'"]+)['"]/g,
  /\bimport\(\s*['"]([^'"]+)['"]\s*\)/g,
  /\bvi\.mock\(\s*['"]([^'"]+)['"]/g,
]

function resolveSpec(fromFile, spec) {
  const target = resolve(dirname(fromFile), spec)
  if (!target.startsWith(srcDir)) return null // 走出 src（node_modules/资源）不算内部依赖
  const base = target.replace(/\?.*$/, '')
  for (const cand of [base, base + '.ts', base + '.d.ts', base + '.vue', join(base, 'index.ts')]) {
    if (existsSync(cand) && statSync(cand).isFile()) return cand
  }
  return null
}

const files = walk(srcDir)
const violations = []
for (const file of files) {
  const layer = classify(relative(srcDir, file))
  const text = readFileSync(file, 'utf8')
  const seen = new Set()
  for (const re of SPEC_RE) {
    for (const m of text.matchAll(re)) {
      const dep = resolveSpec(file, m[1])
      if (!dep || seen.has(dep)) continue
      seen.add(dep)
      const depLayer = classify(relative(srcDir, dep))
      if (!LAYERS[layer].includes(depLayer)) {
        violations.push(`  ${relative(root, file)} (${layer}) → ${relative(root, dep)} (${depLayer})`)
      }
    }
  }
}

if (violations.length) {
  console.error(`分层依赖违规 ${violations.length} 处（规则：只允许向高层依赖）：`)
  for (const v of violations) console.error(v)
  process.exit(1)
}
console.log(`分层依赖检查通过：${files.length} 个文件，0 违规（shared/events/adapters/core/commands/components/views/entry）`)

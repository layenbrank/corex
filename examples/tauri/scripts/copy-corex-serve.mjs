/**
 * 构建前将 corex-serve 复制到 src-tauri/binaries/ 并按 Tauri sidecar 命名。
 *
 * 用法（在 Tauri 项目根目录）：
 *   node scripts/copy-corex-serve.mjs [corex-serve.exe 路径]
 *
 * 默认从环境变量 COREX_SERVE 或 ../corex/target/release/corex-serve.exe 读取。
 */
import { execSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const tauriDir = path.resolve(__dirname, '..', 'src-tauri')
const outDir = path.join(tauriDir, 'binaries')

const ext = process.platform === 'win32' ? '.exe' : ''
const targetTriple = execSync('rustc --print host-tuple', { encoding: 'utf8' }).trim()

const defaultSrc = path.resolve(
	tauriDir,
	'..',
	'..',
	'corex',
	'target',
	'release',
	`corex-serve${ext}`
)

const src = process.argv[2] ?? process.env.COREX_SERVE ?? defaultSrc
const dest = path.join(outDir, `corex-serve-${targetTriple}${ext}`)

if (!fs.existsSync(src)) {
	console.error(`[copy-corex-serve] 源文件不存在: ${src}`)
	console.error('请先执行: cargo build -p corex-serve --release')
	console.error('或通过 COREX_SERVE 环境变量指定路径')
	process.exit(1)
}

fs.mkdirSync(outDir, { recursive: true })
fs.copyFileSync(src, dest)
console.log(`[copy-corex-serve] ${src} -> ${dest}`)

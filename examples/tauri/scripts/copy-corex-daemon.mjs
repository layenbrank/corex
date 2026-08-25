/**
 * 构建前将 corex-daemon 复制到 src-tauri/binaries/ 并按 Tauri sidecar 命名。
 *
 * 用法（在 Tauri 项目根目录）：
 *   node scripts/copy-corex-daemon.mjs [corex-daemon.exe 路径]
 *
 * 默认从环境变量 COREX_DAEMON 或 ../corex/target/release/corex-daemon.exe 读取。
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
	`corex-daemon${ext}`
)

const src = process.argv[2] ?? process.env.COREX_DAEMON ?? defaultSrc
const dest = path.join(outDir, `corex-daemon-${targetTriple}${ext}`)

if (!fs.existsSync(src)) {
	console.error(`[copy-corex-daemon] 源文件不存在: ${src}`)
	console.error('请先执行: cargo build -p corex-daemon --release')
	console.error('或通过 COREX_DAEMON 环境变量指定路径')
	process.exit(1)
}

fs.mkdirSync(outDir, { recursive: true })
fs.copyFileSync(src, dest)
console.log(`[copy-corex-daemon] ${src} -> ${dest}`)

const pdfiumCandidates = [
	process.env.COREX_PDFIUM_DIR
		? path.join(process.env.COREX_PDFIUM_DIR, 'pdfium.dll')
		: null,
	path.resolve(tauriDir, '..', '..', 'corex', 'assets', 'pdfium', targetTriple, 'pdfium.dll'),
	path.resolve(tauriDir, '..', '..', 'assets', 'pdfium', targetTriple, 'pdfium.dll'),
].filter(Boolean)

const pdfiumSrc = pdfiumCandidates.find((p) => p && fs.existsSync(p))
if (pdfiumSrc) {
	const pdfiumDest = path.join(outDir, 'pdfium.dll')
	fs.copyFileSync(pdfiumSrc, pdfiumDest)
	console.log(`[copy-corex-daemon] ${pdfiumSrc} -> ${pdfiumDest}`)
} else {
	console.warn('[copy-corex-daemon] 未找到 pdfium.dll，morph 模块将无法使用（可运行 scripts/download-pdfium.ps1）')
}

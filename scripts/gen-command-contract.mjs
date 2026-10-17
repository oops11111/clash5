import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'

const ROOT = path.resolve(import.meta.dirname, '..')
const MANIFEST = path.join(ROOT, 'src-tauri', 'Cargo.toml')
const OUT = 'src/services/tauri-commands.generated.ts'

if (!fs.existsSync(MANIFEST)) {
  throw new Error(`Tauri Cargo manifest not found: ${MANIFEST}`)
}

execFileSync(
  'cargo',
  [
    'run',
    '--manifest-path',
    MANIFEST,
    '--bin',
    'tauri-command-contract',
    '-q',
  ],
  {
    cwd: path.join(ROOT, 'src-tauri'),
    stdio: 'inherit',
  },
)

execFileSync(
  process.execPath,
  [
    path.join(ROOT, 'node_modules', '@biomejs', 'biome', 'bin', 'biome'),
    'format',
    '--write',
    OUT,
  ],
  { cwd: ROOT, stdio: 'inherit' },
)

console.log(
  `[gen:command-contract] wrote ${OUT} from registered Rust command signatures`,
)

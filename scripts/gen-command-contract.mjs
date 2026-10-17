import { execFileSync } from 'node:child_process'
import path from 'node:path'

const ROOT = path.resolve(import.meta.dirname, '..')
const OUT = 'src/services/tauri-commands.generated.ts'

execFileSync(
  'cargo',
  [
    'run',
    '--manifest-path',
    path.join(ROOT, 'src-tauri', 'Cargo.toml'),
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

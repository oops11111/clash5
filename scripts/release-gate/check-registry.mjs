import { readFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'

const registryPath = new URL('../../RELEASE-GATES.md', import.meta.url)
const requiredGroups = [
  ...Array.from(
    { length: 16 },
    (_, index) => `G-${String(index + 1).padStart(2, '0')}`,
  ),
  'P0-12-CORE-UPDATE',
  'P0-13-BACKUP-RESTORE',
  'G-17…G-22',
  'G-23…G-27',
  'G-28…G-31',
  'G-32…G-34',
  'R19-LINUX-KEYSTORE',
  'R19-NO-CREDENTIALS',
]
const allowedStates = new Set(['OPEN', 'BLOCKED', 'PASS'])

export function validateRegistry(markdown) {
  const rows = markdown
    .split(/\r?\n/u)
    .filter((line) =>
      /^\|\s*(?:G-\d{2}(?:…G-\d{2})?|P0-\d{2}-[A-Z-]+|R19-[A-Z-]+)\s*\|/u.test(
        line,
      ),
    )
    .map((line) =>
      line
        .split('|')
        .slice(1, -1)
        .map((cell) => cell.trim()),
    )

  if (rows.length !== requiredGroups.length) {
    throw new Error(
      `expected ${requiredGroups.length} gate rows, found ${rows.length}`,
    )
  }

  const seen = new Set()
  for (const [index, row] of rows.entries()) {
    if (row.length !== 4 || row.some((cell) => cell.length === 0)) {
      throw new Error(`gate row ${index + 1} must contain four non-empty cells`)
    }
    const [id, scope, evidence, state] = row
    if (id !== requiredGroups[index]) {
      throw new Error(
        `expected ${requiredGroups[index]} at row ${index + 1}, found ${id}`,
      )
    }
    if (seen.has(id)) throw new Error(`duplicate gate ${id}`)
    seen.add(id)
    if (!scope || !evidence)
      throw new Error(`gate ${id} is missing scope or evidence`)
    if (!allowedStates.has(state))
      throw new Error(`gate ${id} has unknown state ${state}`)
    if (
      id === 'G-06' &&
      !['export_rejects_traversal', 'export_rejects_absolute'].every(
        (testName) => evidence.includes(testName),
      )
    ) {
      throw new Error(
        'release gate G-06 must reference both backup export path tests',
      )
    }
  }
  return rows.length
}

export async function verifyNoInAppCoreUpgrade() {
  const sources = [
    '../../src-tauri/src/cmd/clash.rs',
    '../../src-tauri/src/lib.rs',
    '../../src-tauri/src/feat/mod.rs',
    '../../src/services/cmds.ts',
    '../../src/services/tauri-commands.generated.ts',
    '../../src/components/setting/mods/clash-core-viewer.tsx',
  ]
  const forbidden =
    /upgrade_clash_core|upgradeClashCore|upgrade_core|CoreUpgradeReport/u
  for (const source of sources) {
    const contents = await readFile(new URL(source, import.meta.url), 'utf8')
    if (forbidden.test(contents))
      throw new Error(`in-app core upgrade surface remains in ${source}`)
  }
  try {
    await readFile(
      new URL('../../src-tauri/src/feat/core_upgrade.rs', import.meta.url),
    )
    throw new Error('in-app core upgrade module still exists')
  } catch (error) {
    if (error.code !== 'ENOENT') throw error
  }
}

export function validateAutomatedEvidence(backupSource) {
  for (const testName of [
    'export_rejects_traversal',
    'export_rejects_absolute',
  ]) {
    if (!new RegExp(`\\bfn ${testName}\\s*\\(`, 'u').test(backupSource)) {
      throw new Error(`release gate G-06 is missing automated test ${testName}`)
    }
  }
}

export async function verifyAutomatedEvidence() {
  const backupSource = await readFile(
    new URL('../../src-tauri/src/feat/backup.rs', import.meta.url),
    'utf8',
  )
  validateAutomatedEvidence(backupSource)
}

export function validateBackupRestoreSecurity(coreSource, featureSource) {
  if (/danger_accept_invalid_certs\s*\(\s*true\s*\)/u.test(coreSource)) {
    throw new Error('P0-13 WebDAV TLS must validate certificates')
  }
  if (!coreSource.includes('while let Some(chunk) = response.chunk().await?')) {
    throw new Error(
      'P0-13 WebDAV downloads must enforce the archive size limit while streaming',
    )
  }
  for (const evidence of [
    'MAX_BACKUP_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024',
    'MAX_BACKUP_ENTRY_BYTES: u64 = 64 * 1024 * 1024',
    'MAX_BACKUP_ENTRIES: usize = 10_000',
    'entry.by_ref().take(MAX_BACKUP_ENTRY_BYTES + 1)',
    'zip.len() > MAX_BACKUP_ENTRIES',
    'fn is_allowed_backup_path',
    'fn replace_file_staged',
    'rollback_replacements(replacements)',
    'fn restore_allowlist_accepts_only_known_backup_files',
    'fn restore_filename_rejects_path_components',
  ]) {
    if (!featureSource.includes(evidence)) {
      throw new Error(
        `P0-13 backup restore security evidence is missing: ${evidence}`,
      )
    }
  }
}

export async function verifyBackupRestoreSecurity() {
  const [coreSource, featureSource] = await Promise.all([
    readFile(
      new URL('../../src-tauri/src/core/backup.rs', import.meta.url),
      'utf8',
    ),
    readFile(
      new URL('../../src-tauri/src/feat/backup.rs', import.meta.url),
      'utf8',
    ),
  ])
  validateBackupRestoreSecurity(coreSource, featureSource)
}

export function validateWebviewSecurity(config, capability, monacoSource) {
  const security = config.app?.security
  if (!security?.csp || typeof security.csp !== 'object') {
    throw new Error('release gate G-04 requires a production CSP')
  }
  const productionCsp = security.csp
  if (
    productionCsp['default-src'] !== "'self'" ||
    !productionCsp['worker-src']?.includes("'self'") ||
    !productionCsp['worker-src']?.includes('blob:') ||
    !productionCsp['connect-src']?.includes('ipc:') ||
    productionCsp['connect-src']?.includes('https:') ||
    !productionCsp['img-src']?.includes('asset:') ||
    !productionCsp['img-src']?.includes('http://asset.localhost') ||
    !productionCsp['object-src']?.includes("'none'")
  ) {
    throw new Error(
      'release gate G-04 CSP must preserve local workers and Tauri IPC',
    )
  }
  const developmentCsp = security.devCsp
  if (
    !developmentCsp?.['worker-src']?.includes("'self'") ||
    !developmentCsp['worker-src']?.includes('blob:') ||
    !developmentCsp['connect-src']?.includes('ws://localhost:3000')
  ) {
    throw new Error(
      'release gate G-04 development CSP must preserve local workers and Vite HMR',
    )
  }

  const assetScope = security.assetProtocol?.scope?.allow
  if (
    !Array.isArray(assetScope) ||
    assetScope.length === 0 ||
    assetScope.some(
      (path) => path === '**' || !/^\$(?:APPDATA|RESOURCE)\//u.test(path),
    )
  ) {
    throw new Error(
      'release gate G-04 asset scope must stay inside app data and bundled resources',
    )
  }

  const permissions = capability.permissions ?? []
  const filesystemScopes = permissions
    .filter(
      (permission) =>
        typeof permission === 'object' && permission.identifier === 'fs:scope',
    )
    .flatMap((permission) => permission.allow ?? [])
  if (
    filesystemScopes.length === 0 ||
    filesystemScopes.some(
      (path) => path === '**' || !/^\$(?:APPDATA|RESOURCE)\//u.test(path),
    )
  ) {
    throw new Error(
      'release gate G-04 filesystem scopes must stay inside app data and bundled resources',
    )
  }
  for (const permission of [
    'fs:allow-read-file',
    'fs:allow-write-file',
    'dialog:default',
  ]) {
    if (!permissions.includes(permission)) {
      throw new Error(
        `release gate G-04 must preserve system-dialog import/export permission ${permission}`,
      )
    }
  }
  if (!/enableSchemaRequest:\s*false/u.test(monacoSource)) {
    throw new Error(
      'release gate G-04 must disable remote Monaco schema requests',
    )
  }
}

export async function verifyWebviewSecurity() {
  const [configSource, capabilitySource, monacoSource] = await Promise.all([
    readFile(
      new URL('../../src-tauri/tauri.conf.json', import.meta.url),
      'utf8',
    ),
    readFile(
      new URL('../../src-tauri/capabilities/migrated.json', import.meta.url),
      'utf8',
    ),
    readFile(new URL('../../src/services/monaco.ts', import.meta.url), 'utf8'),
  ])
  validateWebviewSecurity(
    JSON.parse(configSource),
    JSON.parse(capabilitySource),
    monacoSource,
  )
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const count = validateRegistry(await readFile(registryPath, 'utf8'))
  await verifyNoInAppCoreUpgrade()
  await verifyAutomatedEvidence()
  await verifyWebviewSecurity()
  await verifyBackupRestoreSecurity()
  process.stdout.write(`Validated ${count} release-gate registry rows.\n`)
}

import assert from 'node:assert/strict'
import { existsSync, readFileSync, readdirSync } from 'node:fs'

import { load } from 'js-yaml'

const source = readFileSync('.github/workflows/release.yml', 'utf8')
const workflow = load(source)
const jobs = workflow.jobs

assert.deepEqual(workflow.on.push.tags, ['v*.*.*'])
assert.equal(workflow.permissions.contents, 'write')
assert.equal(workflow.permissions.attestations, 'write')
assert.equal(workflow.permissions['id-token'], 'write')
assert.deepEqual(
  Object.keys(jobs).sort(),
  [
    'check_tag_version',
    'prepare_release',
    'release',
    'release-for-fixed-webview2',
    'release-for-linux-arm',
  ].sort(),
)
assert.match(source, /RELEASE_BRANCH=dev/)
assert.match(source, /RELEASE_BRANCH=main/)
assert.match(source, /git merge-base --is-ancestor/)
assert.match(source, /EXPECTED_TAG="v\$PKG_VERSION"/)

const draftStep = jobs.prepare_release.steps.find((step) =>
  step.uses?.startsWith('softprops/action-gh-release@'),
)
assert.ok(
  jobs.prepare_release.steps.some((step) =>
    step.run?.includes('already published'),
  ),
  'tag builds must not modify a published release',
)
assert.ok(draftStep, 'a release must first be created as a draft')
assert.equal(draftStep.with.draft, true)
assert.match(draftStep.with.name, /^clash5 /)
const buildStep = jobs.release.steps.find((step) =>
  step.uses?.startsWith('tauri-apps/tauri-action@'),
)
assert.ok(buildStep)
assert.equal(buildStep.with.releaseDraft, true)
assert.ok(
  buildStep.with.releaseId.includes('needs.prepare_release.outputs.release_id'),
)
for (const job of Object.values(jobs)) {
  for (const step of job.steps) {
    if (!step.uses?.startsWith('tauri-apps/tauri-action@')) continue
    assert.equal(step.with.uploadUpdaterJson, false)
    assert.equal(step.with.uploadUpdaterSignatures, false)
    assert.equal(step.with.includeUpdaterJson, undefined)
  }
}

for (const forbidden of [
  /Clash Verge Rev/i,
  /clash-verge-rev\/clash-verge-rev/i,
  /cvr\.dginv\.click/i,
  /draft=false/i,
  /gh\s+release\s+edit/i,
]) {
  assert.doesNotMatch(source, forbidden)
}

for (const workflowPath of [
  '.github/workflows/autobuild.yml',
  '.github/workflows/autobuild-check-test.yml',
  '.github/workflows/check-commit-needs-build.yml',
  '.github/workflows/clean-old-assets.yml',
  '.github/workflows/telegram-notify.yml',
  '.github/workflows/updater.yml',
]) {
  assert.equal(
    existsSync(workflowPath),
    false,
    `${workflowPath} must stay disabled`,
  )
}
const pkg = JSON.parse(readFileSync('package.json', 'utf8'))
assert.match(
  readFileSync('src-tauri/Cargo.toml', 'utf8'),
  /repository = "https:\/\/github\.com\/oops11111\/clash5\.git"/,
)
for (const script of [
  'updater',
  'updater-fixed-webview2',
  'release:autobuild',
  'release:deploytest',
]) {
  assert.equal(pkg.scripts[script], undefined)
}
for (const configPath of [
  'src-tauri/tauri.conf.json',
  'src-tauri/webview2.x64.json',
  'src-tauri/webview2.x86.json',
  'src-tauri/webview2.arm64.json',
]) {
  const tauri = JSON.parse(readFileSync(configPath, 'utf8'))
  assert.deepEqual(
    tauri.plugins.updater.endpoints,
    [],
    `${configPath}: unsigned updater feeds must remain disabled`,
  )
}

for (const filename of readdirSync('.github/ISSUE_TEMPLATE')) {
  if (!filename.endsWith('.yml')) continue
  const templatePath = `.github/ISSUE_TEMPLATE/${filename}`
  assert.doesNotMatch(
    readFileSync(templatePath, 'utf8'),
    /clash-verge-rev|Clash Verge Rev|AutoBuild|CVR Version/i,
    `${templatePath}: issue guidance must point to clash5`,
  )
}

const updateViewer = readFileSync(
  'src/components/setting/mods/update-viewer.tsx',
  'utf8',
)
assert.doesNotMatch(updateViewer, /clash-verge-rev\/clash-verge-rev/i)
assert.match(updateViewer, /github\.com\/oops11111\/clash5\/releases\/tag/)

console.log(
  '[release-draft] tag builds can only upload clash5 candidate assets to a draft',
)

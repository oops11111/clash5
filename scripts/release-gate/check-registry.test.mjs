import test from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { validateRegistry, verifyNoInAppCoreUpgrade } from './check-registry.mjs';

const registry = await readFile(new URL('../../RELEASE-GATES.md', import.meta.url), 'utf8');

test('release gate registry covers all reviewed gate groups', () => {
  assert.equal(validateRegistry(registry), 23);
});

test('the in-app core upgrade command and implementation are absent', async () => {
  await verifyNoInAppCoreUpgrade();
});

test('registry validation fails when a gate row is missing', () => {
  const incomplete = registry.replace(/^\| G-10 .*\r?\n/mu, '');
  assert.throws(() => validateRegistry(incomplete), /expected 23 gate rows/u);
});

test('registry validation fails for an unknown state', () => {
  const invalid = registry.replace('| G-01 |', '| G-01 |').replace(' | OPEN |', ' | READY |');
  assert.throws(() => validateRegistry(invalid), /unknown state/u);
});

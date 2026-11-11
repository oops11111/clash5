import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

const registryPath = new URL('../../RELEASE-GATES.md', import.meta.url);
const requiredGroups = [
  ...Array.from({ length: 16 }, (_, index) => `G-${String(index + 1).padStart(2, '0')}`),
  'G-17…G-22',
  'G-23…G-27',
  'G-28…G-31',
  'G-32…G-34',
  'R19-LINUX-KEYSTORE',
  'R19-NO-CREDENTIALS',
];
const allowedStates = new Set(['OPEN', 'BLOCKED', 'PASS']);

export function validateRegistry(markdown) {
  const rows = markdown
    .split(/\r?\n/u)
    .filter((line) => /^\|\s*(?:G-\d{2}(?:…G-\d{2})?|R19-[A-Z-]+)\s*\|/u.test(line))
    .map((line) => line.split('|').slice(1, -1).map((cell) => cell.trim()));

  if (rows.length !== requiredGroups.length) {
    throw new Error(`expected ${requiredGroups.length} gate rows, found ${rows.length}`);
  }

  const seen = new Set();
  for (const [index, row] of rows.entries()) {
    if (row.length !== 4 || row.some((cell) => cell.length === 0)) {
      throw new Error(`gate row ${index + 1} must contain four non-empty cells`);
    }
    const [id, scope, evidence, state] = row;
    if (id !== requiredGroups[index]) {
      throw new Error(`expected ${requiredGroups[index]} at row ${index + 1}, found ${id}`);
    }
    if (seen.has(id)) throw new Error(`duplicate gate ${id}`);
    seen.add(id);
    if (!scope || !evidence) throw new Error(`gate ${id} is missing scope or evidence`);
    if (!allowedStates.has(state)) throw new Error(`gate ${id} has unknown state ${state}`);
  }
  return rows.length;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const count = validateRegistry(await readFile(registryPath, 'utf8'));
  process.stdout.write(`Validated ${count} release-gate registry rows.\n`);
}

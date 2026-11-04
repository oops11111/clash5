import assert from 'node:assert/strict'
import fs from 'node:fs'

const rules = fs.readFileSync('src-tauri/src/enhance/rules.rs', 'utf8')
const config = fs.readFileSync('src-tauri/src/config/verge.rs', 'utf8')
const enhance = fs.readFileSync('src-tauri/src/enhance/mod.rs', 'utf8')
const frontend = fs.readFileSync('src/components/rule/text-rule-sets.tsx', 'utf8')

assert.match(rules, /Value::from\("proxy"\).*DEFAULT_GROUP/)
assert.match(rules, /Value::from\("size-limit"\).*REMOTE_RULE_SET_MAX_BYTES/)
assert.match(config, /REMOTE_RULE_SET_MAX_BYTES:\s*u64\s*=\s*10 \* 1024 \* 1024/)
assert.match(config, /parsed\.scheme\(\) != "https"/)
assert.match(enhance, /validate_text_rule_sets\(&text_rule_sets\)/)
assert.match(frontend, /test\(url\.trim\(\)\)/)
assert.match(frontend, /https:/)

console.log('rule provider policy audit passed')

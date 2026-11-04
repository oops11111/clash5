import assert from 'node:assert/strict'
import fs from 'node:fs'

const source = fs.readFileSync('src-tauri/src/enhance/script.rs', 'utf8')

assert.match(source, /set_loop_iteration_limit\(MAX_LOOP_ITERATIONS\)/)
assert.match(source, /set_recursion_limit\(MAX_RECURSION_DEPTH\)/)
assert.match(source, /set_stack_size_limit\(MAX_VM_STACK_SIZE\)/)
assert.match(source, /Semaphore::new\(MAX_SCRIPT_CONCURRENCY\)/)
assert.match(source, /acquire_owned\(\)\.await/)
assert.match(source, /SCRIPT_TIMEOUT:.*Duration::from_secs\(5\)/)

console.log('script safety audit passed')

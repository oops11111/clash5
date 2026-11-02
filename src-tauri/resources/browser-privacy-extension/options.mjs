import { normalizeHost, normalizeRules } from './policy.mjs'

const hostInput = document.querySelector('#host')
const profileInput = document.querySelector('#profile')
const status = document.querySelector('#status')
const list = document.querySelector('#rules')
let rules = []

function render() {
  list.replaceChildren()
  for (const rule of rules) {
    const row = document.createElement('tr')
    const host = document.createElement('td')
    host.textContent = rule.host
    const profile = document.createElement('td')
    profile.textContent = rule.profile.replace('_', ' ')
    const action = document.createElement('td')
    const remove = document.createElement('button')
    remove.textContent = 'Remove'
    remove.addEventListener('click', () => { void save(rules.filter((item) => item.host !== rule.host)) })
    action.append(remove)
    row.append(host, profile, action)
    list.append(row)
  }
}

async function save(next) {
  try {
    const normalized = normalizeRules(next)
    const result = await chrome.runtime.sendMessage({ type: 'saveRules', rules: normalized })
    if (!result?.ok) throw new Error(result?.error ?? 'Could not apply the rules.')
    rules = result.rules
    render()
    status.textContent = 'Saved. Reload affected pages.'
  } catch (error) {
    status.textContent = String(error.message ?? error)
  }
}

document.querySelector('#add-rule').addEventListener('submit', (event) => {
  event.preventDefault()
  try {
    const host = normalizeHost(hostInput.value)
    void save([...rules.filter((item) => item.host !== host), { host, profile: profileInput.value }])
  } catch (error) {
    status.textContent = String(error.message ?? error)
  }
})

try {
  const stored = await chrome.storage.local.get(['rules', 'applyError'])
  rules = normalizeRules(stored.rules ?? [])
  render()
  if (stored.applyError) status.textContent = `Last apply failed: ${stored.applyError}`
} catch (error) {
  status.textContent = String(error.message ?? error)
}

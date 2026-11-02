import { buildRules, normalizeRules } from './policy.mjs'

let pending = Promise.resolve()

async function apply(rules) {
  const { scripts, headers } = buildRules(rules)
  const oldScripts = await chrome.scripting.getRegisteredContentScripts()
  const oldHeaders = await chrome.declarativeNetRequest.getDynamicRules()
  try {
    await chrome.scripting.unregisterContentScripts()
    if (scripts.length) await chrome.scripting.registerContentScripts(scripts)
    await chrome.declarativeNetRequest.updateDynamicRules({
      removeRuleIds: oldHeaders.map((rule) => rule.id),
      addRules: headers,
    })
  } catch (error) {
    await chrome.scripting.unregisterContentScripts()
    if (oldScripts.length) await chrome.scripting.registerContentScripts(oldScripts)
    await chrome.declarativeNetRequest.updateDynamicRules({
      removeRuleIds: (await chrome.declarativeNetRequest.getDynamicRules()).map((rule) => rule.id),
      addRules: oldHeaders,
    })
    throw error
  }
}

function serialize(task) {
  const result = pending.then(task)
  pending = result.catch(() => {})
  return result
}

async function restore() {
  try {
    const { rules = [] } = await chrome.storage.local.get('rules')
    await apply(normalizeRules(rules))
    await chrome.storage.local.remove('applyError')
    await chrome.action.setBadgeText({ text: '' })
  } catch (error) {
    await chrome.storage.local.set({ applyError: String(error.message ?? error) })
    await chrome.action.setBadgeText({ text: '!' })
  }
}

chrome.runtime.onInstalled.addListener(() => { void serialize(restore) })
chrome.runtime.onStartup.addListener(() => { void serialize(restore) })
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (sender.id !== chrome.runtime.id || message?.type !== 'saveRules') return
  void serialize(async () => {
    const rules = normalizeRules(message.rules)
    const { rules: previous = [] } = await chrome.storage.local.get('rules')
    await apply(rules)
    try {
      await chrome.storage.local.set({ rules })
    } catch (error) {
      await apply(normalizeRules(previous))
      throw error
    }
    await chrome.storage.local.remove('applyError')
    await chrome.action.setBadgeText({ text: '' })
    return rules
  }).then((rules) => reply({ ok: true, rules }), (error) => reply({ ok: false, error: String(error.message ?? error) }))
  return true
})

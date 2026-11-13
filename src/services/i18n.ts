import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import languageAliasData from '@root/i18n-language-aliases.json'

export const supportedLanguages = [
  'en',
  'ru',
  'zh',
  'fa',
  'tt',
  'id',
  'ar',
  'ko',
  'tr',
  'de',
  'es',
  'jp',
  'zhtw',
  'pt',
]

// The fallback is 'en' on both sides (see DEFAULT_LANGUAGE in
// crates/clash-verge-i18n/src/lib.rs); the key baseline is en too.
export const FALLBACK_LANGUAGE = 'en'
const LANGUAGE_STORAGE_KEY = 'verge-language'

// Language aliases shared with the backend (crates/clash-verge-i18n reads
// the same file): the only place where tags are remapped. Identity tags
// ('jp', 'zh', ...) are not listed; the supported list covers them.
const LANGUAGE_ALIASES: Record<string, string> = languageAliasData.aliases

const normalizeLanguage = (language?: string) =>
  language?.toLowerCase().replace(/_/g, '-')

const resolveLanguage = (language?: string) => {
  const normalized = normalizeLanguage(language)
  if (!normalized) {
    return FALLBACK_LANGUAGE
  }

  // Try the full tag, then progressively shorter prefixes, against the
  // shared alias table first and then the supported list, mirroring the
  // backend resolver; 'ja-JP' resolves to 'jp', 'zh-TW' to 'zhtw'.
  const segments = normalized.split('-')
  for (let i = segments.length; i >= 1; i -= 1) {
    const prefix = segments.slice(0, i).join('-')
    const aliased = LANGUAGE_ALIASES[prefix]
    if (aliased && supportedLanguages.includes(aliased)) {
      return aliased
    }
    if (supportedLanguages.includes(prefix)) {
      return prefix
    }
  }

  return FALLBACK_LANGUAGE
}

export const resolvePreferredLanguage = (
  preference?: string | null,
  systemLanguage?: string,
) =>
  resolveLanguage(
    !preference || preference === 'system'
      ? (systemLanguage ??
          (typeof navigator !== 'undefined' ? navigator.language : undefined))
      : preference,
  )

const getLanguageStorage = () => {
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}

export const cacheLanguage = (language: string) => {
  const storage = getLanguageStorage()
  if (!storage) return

  try {
    storage.setItem(LANGUAGE_STORAGE_KEY, resolveLanguage(language))
  } catch (error) {
    console.warn('[i18n] Failed to cache language:', error)
  }
}

export const getCachedLanguage = () => {
  const storage = getLanguageStorage()
  if (!storage) return undefined

  try {
    const cached = storage.getItem(LANGUAGE_STORAGE_KEY)
    return cached ? resolveLanguage(cached) : undefined
  } catch (error) {
    console.warn('[i18n] Failed to read cached language:', error)
    return undefined
  }
}

type LocaleModule = {
  default: Record<string, unknown>
}

const STARTUP_LANGUAGE_SECTIONS = [
  'layout',
  'home',
  'shared',
  'settings',
  'profiles',
  'proxies',
  'nodePool',
  'chains',
  'tests',
  'rules',
  'connections',
  'logs',
] as const

const localeModules = import.meta.glob<LocaleModule>('@/locales/*/*.json')

const localeLoaders = Object.entries(localeModules).reduce<
  Record<string, Record<string, () => Promise<LocaleModule>>>
>((acc, [path, loader]) => {
  const match = path.match(/[/\\]locales[/\\]([^/\\]+)[/\\]([^/\\]+)\.json$/)
  if (match) {
    const [, language, section] = match
    acc[language] ??= {}
    acc[language][section] = loader
  }
  return acc
}, {})

const loadLanguageSections = async (
  language: string,
  sections: readonly string[],
) => {
  try {
    const entries = await Promise.all(
      sections.map(async (section) => {
        const loader = localeLoaders[language]?.[section]
        if (!loader) {
          throw new Error(
            `Locale loader not found for language "${language}" section "${section}"`,
          )
        }

        const module = await loader()
        return [section, module.default] as const
      }),
    )

    return Object.fromEntries(entries)
  } catch (error) {
    if (language !== FALLBACK_LANGUAGE) {
      console.warn(
        `Failed to load language ${language}, fallback to ${FALLBACK_LANGUAGE}, ${error}`,
      )
      return loadLanguageSections(FALLBACK_LANGUAGE, sections)
    }
    throw error
  }
}

const getLoadedLanguageSections = (language: string) =>
  Object.keys(i18n.getResourceBundle(language, 'translation') ?? {})

i18n.use(initReactI18next).init({
  resources: {},
  lng: FALLBACK_LANGUAGE,
  fallbackLng: FALLBACK_LANGUAGE,
  interpolation: {
    escapeValue: false,
  },
})

const ensureLanguageSections = async (
  sections: string | readonly string[],
  language: string = i18n.language || FALLBACK_LANGUAGE,
) => {
  const targetLanguage = resolveLanguage(language)
  const sectionList = Array.isArray(sections) ? sections : [sections]
  const loadedSections = new Set(getLoadedLanguageSections(targetLanguage))
  const missingSections = sectionList.filter(
    (section) => !loadedSections.has(section),
  )

  if (!missingSections.length) {
    return
  }

  const resources = await loadLanguageSections(targetLanguage, missingSections)
  i18n.addResourceBundle(targetLanguage, 'translation', resources, true, true)
}

export const changeLanguage = async (language: string) => {
  const targetLanguage = resolveLanguage(language)
  const loadedSections = getLoadedLanguageSections(
    i18n.language || FALLBACK_LANGUAGE,
  )

  await ensureLanguageSections(
    [...new Set([...loadedSections, ...STARTUP_LANGUAGE_SECTIONS])],
    targetLanguage,
  )

  await i18n.changeLanguage(targetLanguage)
  cacheLanguage(targetLanguage)
}

export const initializeLanguage = async (
  initialLanguage: string = FALLBACK_LANGUAGE,
) => {
  await changeLanguage(initialLanguage)
}

use rust_i18n::i18n;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

const DEFAULT_LANGUAGE: &str = "en";
i18n!("locales", fallback = "en");

static SUPPORTED_LOCALES: LazyLock<Vec<Cow<'static, str>>> = LazyLock::new(|| rust_i18n::available_locales!());

/// Language aliases shared with the frontend (src/services/i18n.ts reads
/// the same file): the only place where tags are remapped. A malformed
/// table parses to an empty map; the file is also parsed by the frontend
/// build, so a bad file fails CI there.
static LANGUAGE_ALIASES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    const ALIASES_JSON: &str = include_str!("../../../i18n-language-aliases.json");
    serde_json::from_str::<serde_json::Value>(ALIASES_JSON)
        .ok()
        .and_then(|value| value.get("aliases")?.as_object().cloned())
        .map(|aliases| {
            aliases
                .into_iter()
                .filter_map(|(key, value)| value.as_str().map(|target| (key, target.to_string())))
                .collect()
        })
        .unwrap_or_default()
});

#[inline]
fn locale_alias(locale: &str) -> Option<&'static str> {
    LANGUAGE_ALIASES.get(locale).map(String::as_str)
}

#[inline]
fn resolve_supported_language(language: &str) -> Option<Cow<'static, str>> {
    if language.is_empty() {
        return None;
    }
    let normalized = language.to_lowercase().replace('_', "-");
    let segments: Vec<&str> = normalized.split('-').collect();
    for i in (1..=segments.len()).rev() {
        let prefix = segments[..i].join("-");
        if let Some(alias) = locale_alias(&prefix)
            && let Some(found) = SUPPORTED_LOCALES.iter().find(|l| l.eq_ignore_ascii_case(alias))
        {
            return Some(found.clone());
        }
        if let Some(found) = SUPPORTED_LOCALES.iter().find(|l| l.eq_ignore_ascii_case(&prefix)) {
            return Some(found.clone());
        }
    }
    None
}

#[inline]
pub fn current_language(language: Option<&str>) -> Cow<'static, str> {
    language
        .filter(|lang| !lang.is_empty())
        .and_then(resolve_supported_language)
        .unwrap_or_else(system_language)
}

#[inline]
pub fn system_language() -> Cow<'static, str> {
    sys_locale::get_locale()
        .as_deref()
        .and_then(resolve_supported_language)
        .unwrap_or(Cow::Borrowed(DEFAULT_LANGUAGE))
}

#[inline]
pub fn sync_locale(language: Option<&str>) {
    rust_i18n::set_locale(&current_language(language));
}

#[inline]
pub fn set_locale(language: &str) {
    let lang = resolve_supported_language(language).unwrap_or(Cow::Borrowed(DEFAULT_LANGUAGE));
    rust_i18n::set_locale(&lang);
}

#[inline]
pub fn translate(key: &str) -> Cow<'_, str> {
    rust_i18n::t!(key)
}

#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::translate(&$key)
    };
    ($key:expr, $($arg_name:ident = $arg_value:expr),*) => {
        {
            let mut _text = $crate::translate(&$key).into_owned();
            $(
                _text = _text.replace(&format!("{{{}}}", stringify!($arg_name)), &$arg_value);
            )*
            ::std::borrow::Cow::<'static, str>::Owned(_text)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_preference_uses_current_os_locale() {
        assert_eq!(current_language(Some("system")), system_language());
        assert_eq!(current_language(Some("en")), "en");
    }

    #[test]
    fn shared_alias_table_resolves_split_tags() {
        assert_eq!(resolve_supported_language("ja-JP").as_deref(), Some("jp"));
        assert_eq!(resolve_supported_language("ja").as_deref(), Some("jp"));
        assert_eq!(resolve_supported_language("zh-TW").as_deref(), Some("zhtw"));
        assert_eq!(resolve_supported_language("zh-CN").as_deref(), Some("zh"));
        assert_eq!(resolve_supported_language("xx"), None);
        assert_eq!(DEFAULT_LANGUAGE, "en");
    }
}

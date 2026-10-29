use crate::config::Config;
use crate::{
    config::{DEFAULT_PAC, deserialize_encrypted, serialize_encrypted},
    constants::network,
    utils::{dirs, help},
};
use anyhow::Result;
use clash_verge_logging::{Type, logging};
use log::LevelFilter;
use serde::{Deserialize, Serialize};
use smartstring::alias::String;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PerformancePreset {
    #[default]
    Balanced,
    Performance,
    PowerSaving,
}

impl PerformancePreset {
    pub const fn probe_concurrency(self) -> usize {
        match self {
            Self::Balanced => 12,
            Self::Performance => 16,
            Self::PowerSaving => 4,
        }
    }

    pub const fn probe_interval_ms(self) -> u64 {
        match self {
            Self::Balanced => 600_000,
            Self::Performance => 300_000,
            Self::PowerSaving => 1_200_000,
        }
    }
}

/// Where the rules of a text rule set send traffic.
#[derive(Default, Debug, Clone, Copy, Deserialize, Serialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleTarget {
    /// The default proxy: the target of the subscription's final `MATCH` rule.
    #[default]
    Default,
    Direct,
    Reject,
}

/// A user-authored list of `TYPE,content` rule lines.
#[derive(Default, Debug, Clone, Deserialize, Serialize, specta::Type)]
pub struct TextRuleSet {
    pub id: std::string::String,
    pub name: std::string::String,
    pub target: RuleTarget,
    pub content: std::string::String,
}

/// ### `verge.yaml` schema
#[derive(Default, Debug, Clone, Deserialize, Serialize, specta::Type)]
#[serde(default)]
pub struct IVerge {
    /// silent | error | warn | info | debug | trace
    #[specta(type = Option<std::string::String>)]
    pub app_log_level: Option<String>,

    /// Maximum app log size in KiB.
    #[specta(type = Option<specta_typescript::Number>)]
    pub app_log_max_size: Option<u64>,

    #[specta(type = Option<specta_typescript::Number>)]
    pub app_log_max_count: Option<usize>,

    #[specta(type = Option<std::string::String>)]
    pub language: Option<String>,

    /// `light` or `dark` or `system`
    #[specta(type = Option<std::string::String>)]
    pub theme_mode: Option<String>,

    #[specta(type = Option<std::string::String>)]
    pub tray_event: Option<String>,

    #[specta(type = Option<std::string::String>)]
    pub env_type: Option<String>,

    #[specta(type = Option<std::string::String>)]
    pub start_page: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub startup_script: Option<String>,

    pub traffic_graph: Option<bool>,

    /// show memory info (only for Clash Meta)
    pub enable_memory_usage: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_group_icon: Option<bool>,

    pub pause_render_traffic_stats_on_blur: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_tray_icon: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<std::string::String>)]
    pub tray_icon: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<std::string::String>)]
    pub menu_icon: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<Vec<std::string::String>>)]
    pub menu_order: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<std::string::String>)]
    pub proxy_group_tools_position: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<std::string::String>)]
    pub notice_position: Option<String>,

    pub collapse_navbar: Option<bool>,

    pub sysproxy_tray_icon: Option<bool>,

    pub tun_tray_icon: Option<bool>,

    pub enable_tun_mode: Option<bool>,

    pub enable_auto_launch: Option<bool>,

    pub enable_silent_start: Option<bool>,

    pub enable_system_proxy: Option<bool>,

    pub enable_proxy_guard: Option<bool>,

    pub enable_bypass_check: Option<bool>,

    /// Initial DNS override preference for profiles without a saved setting.
    pub enable_dns_settings: Option<bool>,

    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    #[specta(type = std::collections::BTreeMap<std::string::String, super::dns::ProfileDnsSettings>)]
    pub profile_dns_settings: std::collections::BTreeMap<String, super::dns::ProfileDnsSettings>,

    pub use_default_bypass: Option<bool>,

    #[specta(type = Option<std::string::String>)]
    pub system_proxy_bypass: Option<String>,

    #[specta(type = Option<specta_typescript::Number>)]
    pub proxy_guard_duration: Option<u64>,

    pub proxy_auto_config: Option<bool>,

    #[specta(type = Option<std::string::String>)]
    pub pac_file_content: Option<String>,

    #[specta(type = Option<std::string::String>)]
    pub proxy_host: Option<String>,

    pub theme_setting: Option<IVergeTheme>,

    #[specta(type = Option<Vec<std::string::String>>)]
    pub web_ui_list: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<std::string::String>)]
    pub clash_core: Option<String>,

    /// format: {func},{key}
    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<Vec<std::string::String>>)]
    pub hotkeys: Option<Vec<String>>,

    pub enable_global_hotkey: Option<bool>,

    #[specta(type = Option<specta_typescript::Unknown>)]
    pub home_cards: Option<serde_json::Value>,

    /// Seconds between traffic samples shown on the home page.
    #[specta(type = Option<specta_typescript::Number>)]
    pub traffic_refresh_seconds: Option<u64>,

    /// Minutes between automatic IP info refreshes; 0 disables them.
    #[specta(type = Option<specta_typescript::Number>)]
    pub ip_auto_refresh_minutes: Option<u64>,

    /// Text rule sets merged in front of the subscription rules.
    pub text_rule_sets: Option<Vec<TextRuleSet>>,

    pub auto_close_connection: Option<bool>,

    /// Keep the sing-box sidecar alive after its relay becomes idle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub singbox_persistent: Option<bool>,

    pub auto_check_update: Option<bool>,

    pub performance_preset: Option<PerformancePreset>,

    #[specta(type = Option<std::string::String>)]
    pub default_latency_test: Option<String>,

    pub default_latency_timeout: Option<i16>,

    pub enable_auto_delay_detection: Option<bool>,

    /// 自动检测当前节点延迟的间隔（分钟）
    #[specta(type = Option<specta_typescript::Number>)]
    pub auto_delay_detection_interval_minutes: Option<u64>,

    pub enable_builtin_enhanced: Option<bool>,

    pub proxy_layout_column: Option<u8>,

    pub test_list: Option<Vec<IVergeTestItem>>,

    /// 0: 不清理; 1: 1天；2: 7天; 3: 30天; 4: 90天
    pub auto_log_clean: Option<i32>,

    pub enable_auto_backup_schedule: Option<bool>,

    /// Automatic backup interval in hours
    #[specta(type = Option<specta_typescript::Number>)]
    pub auto_backup_interval_hours: Option<u64>,

    pub auto_backup_on_change: Option<bool>,

    /// verge 的各种 port 用于覆盖 clash 的各种 port
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verge_redir_port: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub verge_redir_enabled: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub verge_tproxy_port: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub verge_tproxy_enabled: Option<bool>,

    pub verge_mixed_port: Option<u16>,

    pub verge_socks_port: Option<u16>,

    pub verge_socks_enabled: Option<bool>,

    pub verge_port: Option<u16>,

    pub verge_http_enabled: Option<bool>,

    #[serde(
        serialize_with = "serialize_encrypted",
        deserialize_with = "deserialize_encrypted",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[specta(type = Option<std::string::String>)]
    pub webdav_url: Option<String>,

    #[serde(
        serialize_with = "serialize_encrypted",
        deserialize_with = "deserialize_encrypted",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[specta(type = Option<std::string::String>)]
    pub webdav_username: Option<String>,

    #[serde(
        serialize_with = "serialize_encrypted",
        deserialize_with = "deserialize_encrypted",
        skip_serializing_if = "Option::is_none",
        default
    )]
    #[specta(type = Option<std::string::String>)]
    pub webdav_password: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_tray_speed: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[specta(type = Option<std::string::String>)]
    pub tray_proxy_groups_display_mode: Option<String>,
    pub tray_inline_outbound_modes: Option<bool>,

    pub enable_auto_light_weight_mode: Option<bool>,

    /// 自动进入轻量模式的延迟（分钟）
    #[specta(type = Option<specta_typescript::Number>)]
    pub auto_light_weight_minutes: Option<u64>,

    pub enable_hover_jump_navigator: Option<bool>,

    /// 代理页面自动滚动延迟（毫秒）
    #[specta(type = Option<specta_typescript::Number>)]
    pub hover_jump_navigator_delay: Option<u64>,

    pub enable_external_controller: Option<bool>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize, specta::Type)]
#[serde(default)]
pub struct IVergeTestItem {
    #[specta(type = Option<std::string::String>)]
    pub uid: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub name: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub icon: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub url: Option<String>,
}

#[derive(Default, Debug, Clone, Deserialize, Serialize, specta::Type)]
#[serde(default)]
pub struct IVergeTheme {
    #[specta(type = Option<std::string::String>)]
    pub primary_color: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub secondary_color: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub primary_text: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub secondary_text: Option<String>,

    #[specta(type = Option<std::string::String>)]
    pub info_color: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub error_color: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub warning_color: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub success_color: Option<String>,

    #[specta(type = Option<std::string::String>)]
    pub font_family: Option<String>,
    #[specta(type = Option<std::string::String>)]
    pub css_injection: Option<String>,
}

impl IVerge {
    pub const VALID_CLASH_CORES: &'static [&'static str] = &["verge-mihomo", "verge-mihomo-alpha"];

    pub async fn validate_and_fix_config() -> Result<()> {
        let config_path = dirs::verge_path()?;
        let mut config = match help::read_yaml::<Self>(&config_path).await {
            Ok(config) => config,
            Err(_) => Self::template(),
        };

        let mut needs_fix = false;

        if let Some(ref core) = config.clash_core {
            let core_str = core.trim();
            if core_str.is_empty() || !Self::VALID_CLASH_CORES.contains(&core_str) {
                logging!(
                    warn,
                    Type::Config,
                    "启动时发现无效的clash_core配置: '{}', 将自动修正为 'verge-mihomo'",
                    core
                );
                config.clash_core = Some("verge-mihomo".into());
                needs_fix = true;
            }
        } else {
            logging!(
                info,
                Type::Config,
                "启动时发现未配置clash_core, 将设置为默认值 'verge-mihomo'"
            );
            config.clash_core = Some("verge-mihomo".into());
            needs_fix = true;
        }

        if needs_fix {
            logging!(debug, Type::Config, "正在保存修正后的配置文件...");
            help::save_yaml(&config_path, &config, Some("# clash5 Config")).await?;
            logging!(info, Type::Config, "配置文件修正完成，需要重新加载配置");

            Self::reload_config_after_fix(config).await;
        } else {
            logging!(debug, Type::Config, "clash_core配置验证通过: {:?}", config.clash_core);
        }

        Ok(())
    }

    async fn reload_config_after_fix(updated_config: Self) {
        logging!(
            info,
            Type::Config,
            "内存配置已强制更新，新的clash_core: {:?}",
            &updated_config.clash_core
        );

        let config_draft = Config::verge().await;
        config_draft.edit_draft(|d| {
            *d = updated_config;
        });
        config_draft.apply();
    }

    pub fn get_valid_clash_core(&self) -> String {
        self.clash_core.clone().unwrap_or_else(|| "verge-mihomo".into())
    }

    pub async fn new() -> Self {
        match dirs::verge_path() {
            Ok(path) => match help::read_yaml::<Self>(&path).await {
                Ok(mut config) => {
                    if let Some(start_page) = config.start_page.clone()
                        && start_page == "/home"
                    {
                        config.start_page = Some(String::from("/"));
                    }
                    config
                }
                Err(err) => {
                    logging!(error, Type::Config, "failed to read verge config: {err:#}");
                    Self::template()
                }
            },
            Err(err) => {
                logging!(error, Type::Config, "failed to get verge config path: {err:#}");
                Self::template()
            }
        }
    }

    pub fn template() -> Self {
        Self {
            app_log_max_size: Some(128),
            app_log_max_count: Some(8),
            clash_core: Some("verge-mihomo".into()),
            language: Some("system".into()),
            theme_mode: Some("system".into()),
            #[cfg(not(target_os = "windows"))]
            env_type: Some("bash".into()),
            #[cfg(target_os = "windows")]
            env_type: Some("powershell".into()),
            start_page: Some("/".into()),
            traffic_graph: Some(true),
            enable_memory_usage: Some(true),
            enable_group_icon: Some(true),
            pause_render_traffic_stats_on_blur: Some(true),
            tray_icon: cfg!(target_os = "macos").then(|| "monochrome".into()),
            menu_icon: Some("monochrome".into()),
            notice_position: Some("top-right".into()),
            collapse_navbar: Some(false),
            common_tray_icon: Some(false),
            sysproxy_tray_icon: Some(false),
            tun_tray_icon: Some(false),
            enable_auto_launch: Some(false),
            enable_silent_start: Some(false),
            enable_hover_jump_navigator: Some(true),
            hover_jump_navigator_delay: Some(280),
            enable_system_proxy: Some(false),
            proxy_auto_config: Some(false),
            pac_file_content: Some(DEFAULT_PAC.into()),
            proxy_host: Some("127.0.0.1".into()),
            verge_redir_port: (!cfg!(target_os = "windows")).then_some(7895),
            verge_redir_enabled: (!cfg!(target_os = "windows")).then_some(false),
            verge_tproxy_port: cfg!(target_os = "linux").then_some(7896),
            verge_tproxy_enabled: cfg!(target_os = "linux").then_some(false),
            verge_mixed_port: Some(network::ports::DEFAULT_MIXED),
            verge_socks_port: Some(7898),
            verge_socks_enabled: Some(false),
            verge_port: Some(7899),
            verge_http_enabled: Some(false),
            enable_proxy_guard: Some(false),
            enable_bypass_check: Some(true),
            use_default_bypass: Some(true),
            proxy_guard_duration: Some(30),
            auto_close_connection: Some(true),
            singbox_persistent: Some(false),
            auto_check_update: Some(false),
            performance_preset: Some(PerformancePreset::Balanced),
            enable_builtin_enhanced: Some(true),
            auto_log_clean: Some(2), // 1: 1天, 2: 7天, 3: 30天, 4: 90天
            enable_auto_backup_schedule: Some(false),
            auto_backup_interval_hours: Some(24),
            auto_backup_on_change: Some(true),
            webdav_url: None,
            webdav_username: None,
            webdav_password: None,
            enable_tray_speed: cfg!(target_os = "macos").then_some(false),
            tray_proxy_groups_display_mode: Some("default".into()),
            tray_inline_outbound_modes: Some(false),
            enable_global_hotkey: Some(true),
            enable_auto_light_weight_mode: Some(false),
            auto_light_weight_minutes: Some(10),
            enable_dns_settings: Some(false),
            home_cards: None,
            traffic_refresh_seconds: Some(2),
            ip_auto_refresh_minutes: Some(0),
            enable_external_controller: Some(false),
            ..Self::default()
        }
    }

    pub async fn save_file(&self) -> Result<()> {
        help::save_yaml(&dirs::verge_path()?, &self, Some("# clash5 Config")).await
    }

    #[allow(clippy::cognitive_complexity)]
    pub fn patch_config(&mut self, patch: &Self) {
        macro_rules! patch {
            ($key: tt) => {
                if patch.$key.is_some() {
                    self.$key = patch.$key.clone();
                }
            };
        }

        patch!(app_log_level);
        patch!(app_log_max_size);
        patch!(app_log_max_count);

        patch!(language);
        patch!(theme_mode);
        patch!(tray_event);
        patch!(env_type);
        patch!(start_page);
        patch!(startup_script);
        patch!(traffic_graph);
        patch!(enable_memory_usage);
        patch!(enable_group_icon);
        patch!(pause_render_traffic_stats_on_blur);
        #[cfg(target_os = "macos")]
        patch!(tray_icon);
        patch!(menu_icon);
        patch!(menu_order);
        patch!(proxy_group_tools_position);
        patch!(notice_position);
        patch!(collapse_navbar);
        patch!(common_tray_icon);
        patch!(sysproxy_tray_icon);
        patch!(tun_tray_icon);

        patch!(enable_tun_mode);
        patch!(enable_auto_launch);
        patch!(enable_silent_start);
        patch!(enable_hover_jump_navigator);
        patch!(hover_jump_navigator_delay);
        #[cfg(not(target_os = "windows"))]
        patch!(verge_redir_port);
        #[cfg(not(target_os = "windows"))]
        patch!(verge_redir_enabled);
        #[cfg(target_os = "linux")]
        patch!(verge_tproxy_port);
        #[cfg(target_os = "linux")]
        patch!(verge_tproxy_enabled);
        patch!(verge_mixed_port);
        patch!(verge_socks_port);
        patch!(verge_socks_enabled);
        patch!(verge_port);
        patch!(verge_http_enabled);
        patch!(enable_system_proxy);
        patch!(enable_proxy_guard);
        patch!(enable_bypass_check);
        patch!(use_default_bypass);
        patch!(system_proxy_bypass);
        patch!(proxy_guard_duration);
        patch!(proxy_auto_config);
        patch!(pac_file_content);
        patch!(proxy_host);
        patch!(theme_setting);
        patch!(web_ui_list);
        patch!(clash_core);
        patch!(hotkeys);
        patch!(enable_global_hotkey);

        patch!(auto_close_connection);
        patch!(singbox_persistent);
        patch!(auto_check_update);
        patch!(performance_preset);
        patch!(default_latency_test);
        patch!(default_latency_timeout);
        patch!(enable_auto_delay_detection);
        patch!(auto_delay_detection_interval_minutes);
        patch!(enable_builtin_enhanced);
        patch!(proxy_layout_column);
        patch!(test_list);
        patch!(auto_log_clean);
        patch!(enable_auto_backup_schedule);
        patch!(auto_backup_interval_hours);
        patch!(auto_backup_on_change);

        patch!(webdav_url);
        patch!(webdav_username);
        patch!(webdav_password);
        #[cfg(target_os = "macos")]
        patch!(enable_tray_speed);
        patch!(tray_proxy_groups_display_mode);
        patch!(tray_inline_outbound_modes);
        patch!(enable_auto_light_weight_mode);
        patch!(auto_light_weight_minutes);
        patch!(enable_dns_settings);
        patch!(home_cards);
        patch!(traffic_refresh_seconds);
        patch!(ip_auto_refresh_minutes);
        patch!(text_rule_sets);
        patch!(enable_external_controller);
    }

    pub fn get_log_level(&self) -> LevelFilter {
        if let Some(level) = self.app_log_level.as_ref() {
            match level.to_lowercase().as_str() {
                "silent" => LevelFilter::Off,
                "error" => LevelFilter::Error,
                "warn" => LevelFilter::Warn,
                "info" => LevelFilter::Info,
                "debug" => LevelFilter::Debug,
                "trace" => LevelFilter::Trace,
                _ => LevelFilter::Info,
            }
        } else {
            LevelFilter::Info
        }
    }
}

#[cfg(test)]
mod performance_preset_tests {
    use super::*;

    #[test]
    fn preset_persists_in_verge_config() -> Result<(), serde_yaml_ng::Error> {
        let mut config = IVerge::template();
        assert_eq!(config.performance_preset, Some(PerformancePreset::Balanced));
        config.patch_config(&IVerge {
            performance_preset: Some(PerformancePreset::PowerSaving),
            ..IVerge::default()
        });
        let yaml = serde_yaml_ng::to_string(&config)?;
        let restored: IVerge = serde_yaml_ng::from_str(&yaml)?;
        assert_eq!(restored.performance_preset, Some(PerformancePreset::PowerSaving));
        Ok(())
    }
}

use crate::core::notify::NoticeStatus;
use crate::core::notify::{Refresh, announce};
use crate::{
    cmd,
    config::{
        Config, PrfItem, PrfOption,
        profiles::{BindingSync, profiles_update_item_safe},
    },
    core::{CoreManager, handle, tray, validate::ValidationOutcome},
    utils::help::{mask_err, mask_url},
};
use anyhow::{Result, bail};
use clash_verge_logging::{Type, logging, logging_error};
use smartstring::alias::String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UpdateStrategy {
    Direct,
    Clash,
    System,
}

fn select_update_strategy(option: Option<&PrfOption>, core_running: bool) -> UpdateStrategy {
    match (
        option.and_then(|option| option.self_proxy),
        option.and_then(|option| option.with_proxy),
    ) {
        (Some(true), _) => UpdateStrategy::Clash,
        (_, Some(true)) => UpdateStrategy::System,
        (Some(false), _) | (_, Some(false)) => UpdateStrategy::Direct,
        (None, None) if core_running => UpdateStrategy::Clash,
        (None, None) => UpdateStrategy::Direct,
    }
}

/// Toggle proxy profile
pub async fn toggle_proxy_profile(profile_index: String) {
    if let Err(err) = cmd::patch_profiles_config_by_profile_index(profile_index.clone()).await {
        logging!(
            error,
            Type::Config,
            "toggle proxy profile {profile_index} failed: {err}"
        );
    }
}

/// Tell the profile which node this group is on now.
///
/// The core is already switched by the time this runs, so failing to record is not a reason to
/// report the switch as failed — but it is worth a log line, because what the profile holds is
/// what gets re-applied the next time a core starts.
async fn record_switched_node(group_name: &str, proxy_name: &str) {
    if let Err(error) = crate::config::profiles::record_selected_node(group_name, proxy_name).await {
        logging!(
            warn,
            Type::Config,
            "切换代理成功但未能记录到配置: {} -> {}, 错误: {error:#}",
            group_name,
            proxy_name
        );
    }
}

pub async fn switch_proxy_node(group_name: &str, proxy_name: &str) {
    match handle::Handle::mihomo()
        .select_node_for_group(group_name, proxy_name)
        .await
    {
        Ok(_) => {
            record_switched_node(group_name, proxy_name).await;
            announce(Refresh::Proxies);
            let _ = tray::Tray::global().update_menu().await;
            return;
        }
        Err(err) => {
            logging!(error, Type::Tray, "切换代理失败: {err:?}");
        }
    }

    match handle::Handle::mihomo()
        .select_node_for_group(group_name, proxy_name)
        .await
    {
        Ok(_) => {
            logging!(info, Type::Tray, "代理切换回退成功: {} -> {}", group_name, proxy_name);
            record_switched_node(group_name, proxy_name).await;
            let _ = tray::Tray::global().update_menu().await;
        }
        Err(err) => {
            logging!(error, Type::Tray, "代理切换最终失败: {err:?}");
        }
    }
}

async fn should_update_profile(uid: &String, ignore_auto_update: bool) -> Result<Option<(String, Option<PrfOption>)>> {
    let profiles = Config::profiles().await;
    let profiles = profiles.latest_arc();
    let item = profiles.get_item(uid)?;
    let is_remote = item.itype.as_ref().is_some_and(|s| s == "remote");

    if !is_remote {
        logging!(info, Type::Config, "[订阅更新] 不是远程订阅，跳过更新");
        Ok(None)
    } else if item.url.is_none() {
        logging!(warn, Type::Config, "[订阅更新] 缺少URL，无法更新");
        bail!("failed to get the profile item url");
    } else if !ignore_auto_update && !item.option.as_ref().and_then(|o| o.allow_auto_update).unwrap_or(true) {
        logging!(info, Type::Config, "[订阅更新] 禁止自动更新，跳过更新");
        Ok(None)
    } else {
        logging!(
            info,
            Type::Config,
            "[订阅更新] 远程订阅，URL: {}",
            mask_url(
                item.url
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Profile URL is None"))?
            )
        );
        Ok(Some((
            item.url.clone().ok_or_else(|| anyhow::anyhow!("Profile URL is None"))?,
            item.option.clone(),
        )))
    }
}

#[tracing::instrument(skip_all, level = "info", fields(uid = %uid, strategy = tracing::field::Empty, configured_elapsed = tracing::field::Empty, clash_elapsed = tracing::field::Empty, system_elapsed = tracing::field::Empty))]
async fn perform_profile_update(
    uid: &String,
    url: &String,
    opt: Option<&PrfOption>,
    option: Option<&PrfOption>,
    is_mannual_trigger: bool,
) -> Result<()> {
    let merged_opt = PrfOption::merge(opt, option);
    let profiles = Config::profiles().await;
    let profiles_arc = profiles.latest_arc();
    let profile_name = profiles_arc
        .get_item(uid)
        .ok()
        .and_then(|item| item.name.clone())
        .unwrap_or_else(|| String::from("UnKnown Profile"));

    let strategy = select_update_strategy(
        merged_opt.as_ref(),
        !matches!(
            *CoreManager::global().get_running_mode(),
            crate::core::manager::RunningMode::NotRunning
        ),
    );
    let strategy_name = match strategy {
        UpdateStrategy::Direct => "direct",
        UpdateStrategy::Clash => "clash",
        UpdateStrategy::System => "system",
    };
    tracing::Span::current().record("strategy", strategy_name);
    let started = std::time::Instant::now();
    let mut options = merged_opt.unwrap_or_default();
    match strategy {
        UpdateStrategy::Direct => {
            options.self_proxy = Some(false);
            options.with_proxy = Some(false);
        }
        UpdateStrategy::Clash => {
            options.self_proxy = Some(true);
            options.with_proxy = Some(false);
        }
        UpdateStrategy::System => {
            options.self_proxy = Some(false);
            options.with_proxy = Some(true);
        }
    }
    let result = PrfItem::from_url(url, None, None, Some(&options)).await;
    let elapsed_field = match strategy {
        UpdateStrategy::Direct => "configured_elapsed",
        UpdateStrategy::Clash => "clash_elapsed",
        UpdateStrategy::System => "system_elapsed",
    };
    tracing::Span::current().record(elapsed_field, tracing::field::debug(started.elapsed()));
    match &result {
        Ok(_) => logging!(
            info,
            Type::Config,
            "[订阅更新] 使用 {strategy_name} 通道更新订阅配置成功"
        ),
        Err(error) => logging!(
            warn,
            Type::Config,
            "[订阅更新] 使用 {strategy_name} 通道更新失败: {}",
            mask_err(&format!("{error:#}"))
        ),
    }
    let result = result.map(|item| (strategy, item));
    let last_err = match result {
        Ok((strategy, mut item)) => {
            // A persistence error must not replay a successful download with another transport.
            if let BindingSync::Failed(error) = profiles_update_item_safe(uid, &mut item).await? {
                let message = format!("Subscription saved, but clash5 node bindings were not synchronized: {error}");
                logging!(warn, Type::Config, "{message}");
                handle::Handle::notice(NoticeStatus::UpdateFailed, message.as_str());
            }
            if matches!(strategy, UpdateStrategy::Clash | UpdateStrategy::System) {
                handle::Handle::notice(NoticeStatus::UpdateWithClashProxy, profile_name.as_str());
            }
            return Ok(());
        }
        Err(error) => error,
    };

    let last_err = mask_err(&last_err.to_string());
    if is_mannual_trigger {
        handle::Handle::notice(
            NoticeStatus::UpdateFailedEvenWithClash,
            format!("{profile_name} - {last_err}"),
        );
    }
    bail!(last_err)
}

#[tracing::instrument(skip_all, level = "info", fields(uid = %uid, manual = is_mannual_trigger))]
pub async fn update_profile(uid: &String, option: Option<&PrfOption>, is_mannual_trigger: bool) -> Result<()> {
    let _profile_write_guard = crate::config::profiles::PROFILE_WRITE_LOCK.lock().await;
    let url_opt = should_update_profile(uid, is_mannual_trigger).await?;

    let profile_persisted = match url_opt {
        Some((url, opt)) => {
            perform_profile_update(uid, &url, opt.as_ref(), option, is_mannual_trigger).await?;
            true
        }
        None => false,
    };
    let profiles = Config::profiles().await;
    let is_current = profiles.latest_arc().current.as_ref() == Some(uid);
    let should_refresh = is_current || (!profile_persisted && is_mannual_trigger);

    if should_refresh {
        logging!(debug, Type::Config, "[订阅更新] 更新内核配置");
        match CoreManager::global().update_config_with_force(is_mannual_trigger).await {
            Ok(outcome) if outcome.is_valid() => {
                logging_error!(Type::Config, Config::sync_dns_override().await);
                announce(Refresh::Clash);
            }
            Ok(outcome @ (ValidationOutcome::Skipped { .. } | ValidationOutcome::Busy)) if !is_mannual_trigger => {
                logging!(info, Type::Config, "[订阅更新] 本次配置刷新已跳过: {}", outcome);
            }
            result => {
                let message = match result {
                    Ok(outcome) => outcome.to_string(),
                    Err(err) => err.to_string(),
                };
                let message = mask_err(&message);
                let message = if profile_persisted {
                    format!("订阅已保存，但未能确认已应用到内核: {message}")
                } else {
                    message
                };
                logging!(error, Type::Config, "[订阅更新] 更新失败: {}", message);
                handle::Handle::notice(NoticeStatus::UpdateFailed, message.as_str());
                bail!(message);
            }
        }
    }

    Ok(())
}

/// 增强配置
pub async fn enhance_profiles() -> Result<ValidationOutcome> {
    let outcome = CoreManager::global().update_config_forced().await?;
    if outcome.is_valid() {
        logging_error!(Type::Config, Config::sync_dns_override().await);
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::{UpdateStrategy, select_update_strategy};
    use crate::config::PrfOption;

    #[test]
    fn automatic_updates_use_clash_only_when_the_core_is_running() {
        assert_eq!(select_update_strategy(None, true), UpdateStrategy::Clash);
        assert_eq!(select_update_strategy(None, false), UpdateStrategy::Direct);
    }

    #[test]
    fn explicit_transport_choices_never_fall_back_to_another_channel() {
        assert_eq!(
            select_update_strategy(
                Some(&PrfOption {
                    with_proxy: Some(true),
                    ..Default::default()
                }),
                false,
            ),
            UpdateStrategy::System
        );
        assert_eq!(
            select_update_strategy(
                Some(&PrfOption {
                    self_proxy: Some(true),
                    ..Default::default()
                }),
                false,
            ),
            UpdateStrategy::Clash
        );
        assert_eq!(
            select_update_strategy(
                Some(&PrfOption {
                    with_proxy: Some(false),
                    ..Default::default()
                }),
                true,
            ),
            UpdateStrategy::Direct
        );
    }
}

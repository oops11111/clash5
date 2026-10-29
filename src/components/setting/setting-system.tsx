import React, { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { DialogRef, Switch, TooltipIcon } from '@/components/base'
import ProxyControlSwitches from '@/components/shared/proxy-control-switches'
import { useVerge } from '@/hooks/use-verge'

import { GuardState } from './mods/guard-state'
import { SettingItem, SettingSection } from './mods/setting-comp'
import { SysproxyViewer } from './mods/sysproxy-viewer'
import { TunViewer } from './mods/tun-viewer'

interface Props {
  section: SettingSection
  onError?: (err: Error) => void
}

const SettingSystem = ({ section, onError }: Props) => {
  const { t } = useTranslation()

  const { verge, mutateVerge, patchVerge } = useVerge()

  const { enable_auto_launch, enable_silent_start } = verge ?? {}

  const sysproxyRef = useRef<DialogRef>(null)
  const tunRef = useRef<DialogRef>(null)

  const onSwitchFormat = (
    _e: React.ChangeEvent<HTMLInputElement>,
    value: boolean,
  ) => value
  const onChangeData = (patch: Partial<IVergeConfig>) => {
    mutateVerge({ ...verge, ...patch }, false)
  }

  return (
    <>
      <SysproxyViewer ref={sysproxyRef} />
      <TunViewer ref={tunRef} />

      {section === 'system' && (
        <ProxyControlSwitches
          label={t('settings.sections.system.toggles.tunMode')}
          onError={onError}
        />
      )}

      {section === 'system' && (
        <ProxyControlSwitches
          label={t('settings.sections.system.toggles.systemProxy')}
          onError={onError}
        />
      )}

      {section === 'system' && (
        <SettingItem label={t('settings.sections.system.fields.autoLaunch')}>
          <GuardState
            value={enable_auto_launch ?? false}
            valueProps="checked"
            onCatch={onError}
            onFormat={onSwitchFormat}
            onChange={(e) => {
              onChangeData({ enable_auto_launch: e })
            }}
            onGuard={async (e) => {
              try {
                // 先触发UI更新立即看到反馈
                onChangeData({ enable_auto_launch: e })
                await patchVerge({ enable_auto_launch: e })
                return Promise.resolve()
              } catch (error) {
                // 如果出错，恢复原始状态
                onChangeData({ enable_auto_launch: !e })
                return Promise.reject(error)
              }
            }}
          >
            <Switch edge="end" />
          </GuardState>
        </SettingItem>
      )}

      {section === 'system' && (
        <SettingItem
          label={t('settings.sections.system.fields.silentStart')}
          extra={
            <TooltipIcon
              title={t('settings.sections.system.tooltips.silentStart')}
              sx={{ opacity: '0.7' }}
            />
          }
        >
          <GuardState
            value={enable_silent_start ?? false}
            valueProps="checked"
            onCatch={onError}
            onFormat={onSwitchFormat}
            onChange={(e) => onChangeData({ enable_silent_start: e })}
            onGuard={(e) => patchVerge({ enable_silent_start: e })}
          >
            <Switch edge="end" />
          </GuardState>
        </SettingItem>
      )}
    </>
  )
}

export default SettingSystem

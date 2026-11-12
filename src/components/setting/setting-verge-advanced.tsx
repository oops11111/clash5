import { ContentCopyRounded } from '@mui/icons-material'
import ExpandMoreRounded from '@mui/icons-material/ExpandMoreRounded'
import {
  Accordion,
  AccordionDetails,
  AccordionSummary,
  List,
  Stack,
  Typography,
} from '@mui/material'
import { useCallback, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BaseDialog, DialogRef, TooltipIcon } from '@/components/base'
import {
  exitApp,
  exportDiagnosticInfo,
  openAppDir,
  openCoreDir,
  openDevTools,
  openLogsDir,
} from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'
import { version } from '@root/package.json'

import { BackupViewer } from './mods/backup-viewer'
import { ConfigViewer } from './mods/config-viewer'
import { HotkeyViewer } from './mods/hotkey-viewer'
import { LayoutViewer } from './mods/layout-viewer'
import { LiteModeViewer } from './mods/lite-mode-viewer'
import { MiscViewer } from './mods/misc-viewer'
import { SettingItem } from './mods/setting-comp'
import { ThemeViewer } from './mods/theme-viewer'
import { UpdateViewer } from './mods/update-viewer'

interface Props {
  onError?: (err: Error) => void
}

const SettingVergeAdvanced = ({ onError: _ }: Props) => {
  const { t } = useTranslation()

  const configRef = useRef<DialogRef>(null)
  const hotkeyRef = useRef<DialogRef>(null)
  const miscRef = useRef<DialogRef>(null)
  const themeRef = useRef<DialogRef>(null)
  const layoutRef = useRef<DialogRef>(null)
  const updateRef = useRef<DialogRef>(null)
  const backupRef = useRef<DialogRef>(null)
  const liteModeRef = useRef<DialogRef>(null)
  const [confirmExit, setConfirmExit] = useState(false)

  const onExportDiagnosticInfo = useCallback(async () => {
    await mutate(() => exportDiagnosticInfo(), {
      id: 'export-diagnostic-info',
      errorNotice: false,
    })
    showNotice.success('shared.feedback.notifications.common.copySuccess', 1000)
  }, [])

  const copyVersion = useCallback(() => {
    navigator.clipboard.writeText(`v${version}`).then(() => {
      showNotice.success(
        'settings.components.verge.advanced.notifications.versionCopied',
        1000,
      )
    })
  }, [])

  return (
    <>
      <Accordion
        data-testid="advanced-settings-accordion"
        defaultExpanded={false}
        disableGutters
        elevation={0}
        square
        sx={{
          bgcolor: 'transparent',
          '&:before': { display: 'none' },
        }}
      >
        <AccordionSummary
          id="advanced-settings-heading"
          aria-controls="advanced-settings-panel"
          expandIcon={<ExpandMoreRounded />}
          sx={{ minHeight: 64, px: 2 }}
        >
          <Stack spacing={0.25}>
            <Typography variant="subtitle1" sx={{ fontWeight: 700 }}>
              {t('settings.components.verge.advanced.title')}
            </Typography>
            <Typography variant="body2" color="text.secondary">
              {t('settings.components.verge.advanced.description')}
            </Typography>
          </Stack>
        </AccordionSummary>
        <AccordionDetails sx={{ px: 0, pt: 0 }}>
          <List
            id="advanced-settings-panel"
            aria-labelledby="advanced-settings-heading"
          >
            <ThemeViewer ref={themeRef} />
            <ConfigViewer ref={configRef} />
            <HotkeyViewer ref={hotkeyRef} />
            <MiscViewer ref={miscRef} />
            <LayoutViewer ref={layoutRef} />
            <UpdateViewer ref={updateRef} />
            <BackupViewer ref={backupRef} />
            <LiteModeViewer ref={liteModeRef} />

            <SettingItem
              onClick={() => backupRef.current?.open()}
              label={t(
                'settings.components.verge.advanced.fields.backupSetting',
              )}
              extra={
                <TooltipIcon
                  title={t(
                    'settings.components.verge.advanced.tooltips.backupInfo',
                  )}
                  sx={{ opacity: '0.7' }}
                />
              }
            />

            <SettingItem
              onClick={() => configRef.current?.open()}
              label={t(
                'settings.components.verge.advanced.fields.runtimeConfig',
              )}
            />

            <SettingItem
              onClick={openAppDir}
              label={t('settings.components.verge.advanced.fields.openConfDir')}
              extra={
                <TooltipIcon
                  title={t(
                    'settings.components.verge.advanced.tooltips.openConfDir',
                  )}
                  sx={{ opacity: '0.7' }}
                />
              }
            />

            <SettingItem
              onClick={openCoreDir}
              label={t('settings.components.verge.advanced.fields.openCoreDir')}
            />

            <SettingItem
              onClick={openLogsDir}
              label={t('settings.components.verge.advanced.fields.openLogsDir')}
            />

            <SettingItem
              onClick={() => {
                void mutate(() => openDevTools(), {
                  id: 'open-devtools',
                  errorNotice: false,
                }).catch(() => {})
              }}
              label={t(
                'settings.components.verge.advanced.fields.openDevTools',
              )}
            />

            <SettingItem
              testId="advanced-settings-lite-mode"
              label={t(
                'settings.components.verge.advanced.fields.liteModeSettings',
              )}
              extra={
                <TooltipIcon
                  title={t(
                    'settings.components.verge.advanced.tooltips.liteMode',
                  )}
                  sx={{ opacity: '0.7' }}
                />
              }
              onClick={() => liteModeRef.current?.open()}
            />

            <SettingItem
              testId="advanced-settings-exit"
              onClick={() => {
                setConfirmExit(true)
              }}
              label={t('settings.components.verge.advanced.fields.exit')}
            />

            <SettingItem
              label={t(
                'settings.components.verge.advanced.fields.exportDiagnostics',
              )}
              extra={
                <TooltipIcon
                  icon={ContentCopyRounded}
                  onClick={onExportDiagnosticInfo}
                />
              }
            ></SettingItem>

            <SettingItem
              label={t(
                'settings.components.verge.advanced.fields.vergeVersion',
              )}
              extra={
                <TooltipIcon
                  icon={ContentCopyRounded}
                  onClick={copyVersion}
                  title={t(
                    'settings.components.verge.advanced.actions.copyVersion',
                  )}
                />
              }
            >
              <Typography sx={{ py: '7px', pr: 1 }}>v{version}</Typography>
            </SettingItem>
          </List>
        </AccordionDetails>
      </Accordion>

      <BaseDialog
        testId="advanced-settings-exit-confirmation"
        open={confirmExit}
        title={t('settings.components.verge.advanced.confirmExitTitle')}
        okBtn={t('shared.actions.confirm')}
        cancelBtn={t('shared.actions.cancel')}
        onClose={() => setConfirmExit(false)}
        onCancel={() => setConfirmExit(false)}
        onOk={() => {
          setConfirmExit(false)
          void mutate(() => exitApp(), {
            id: 'exit-app',
            errorNotice: false,
          }).catch(() => {})
        }}
      >
        <Typography>
          {t('settings.components.verge.advanced.confirmExitMessage')}
        </Typography>
      </BaseDialog>
    </>
  )
}

export default SettingVergeAdvanced

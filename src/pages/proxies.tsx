import { LanOutlined } from '@mui/icons-material'
import { Box, Button, ButtonGroup } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { BasePage } from '@/components/base'
import { ProviderButton } from '@/components/proxy/provider-button'
import { ProxyGroups } from '@/components/proxy/proxy-groups'
import {
  useAppRefreshers,
  useClashConfigData,
} from '@/providers/app-data-context'
import { patchClashMode } from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'

const MODES = ['rule', 'global', 'direct'] as const
type Mode = (typeof MODES)[number]
const MODE_SET = new Set<string>(MODES)
const isMode = (value: unknown): value is Mode =>
  typeof value === 'string' && MODE_SET.has(value)

const ProxyPage = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { clashConfig } = useClashConfigData()
  const { refreshClashConfig } = useAppRefreshers()
  const normalizedMode = clashConfig?.mode?.toLowerCase()
  const curMode = isMode(normalizedMode) ? normalizedMode : undefined

  const onChangeMode = useLockFn(async (mode: Mode) => {
    try {
      await mutate(() => patchClashMode(mode), {
        id: 'patch-clash-mode',
        errorNotice: false,
      })
      refreshClashConfig()
    } catch (error) {
      showNotice.error(error)
    }
  })

  useEffect(() => {
    if (normalizedMode && !isMode(normalizedMode)) onChangeMode('rule')
  }, [normalizedMode, onChangeMode])

  return (
    <BasePage
      full
      contentStyle={{ height: '100%' }}
      title={t('proxies.page.title.default')}
      header={
        <Box
          sx={{
            display: 'flex',
            alignItems: 'center',
            gap: 1,
            flexWrap: 'wrap',
          }}
        >
          <ProviderButton />
          <ButtonGroup size="small">
            {MODES.map((mode) => (
              <Button
                key={mode}
                variant={mode === curMode ? 'contained' : 'outlined'}
                onClick={() => onChangeMode(mode)}
                sx={{ textTransform: 'capitalize' }}
              >
                {t(`proxies.page.modes.${mode}`)}
              </Button>
            ))}
          </ButtonGroup>
          <Button
            size="small"
            variant="outlined"
            onClick={() => navigate('/chains')}
            sx={{ ml: 1 }}
            startIcon={<LanOutlined fontSize="small" />}
          >
            {t('layout.components.navigation.tabs.chains')}
          </Button>
        </Box>
      }
    >
      <ProxyGroups mode={curMode ?? 'rule'} />
    </BasePage>
  )
}

export default ProxyPage

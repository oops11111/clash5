import LanOutlined from '@mui/icons-material/LanOutlined'
import {
  Box,
  Button,
  Chip,
  CircularProgress,
  Stack,
  Typography,
} from '@mui/material'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate } from 'react-router'

import { getClash5ChainWorkspace } from '@/services/cmds'
import type { ChainWorkspace } from '@/types/clash5-chain'

import { EnhancedCard } from './enhanced-card'

interface ChainCardProps {
  profileId: string | null
}

interface ChainLoadState {
  profileId: string | null | undefined
  workspace: ChainWorkspace | null
  failed: boolean
}

export const HomeChainCard = ({ profileId }: ChainCardProps) => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [loadState, setLoadState] = useState<ChainLoadState>({
    profileId: undefined,
    workspace: null,
    failed: false,
  })

  useEffect(() => {
    let disposed = false

    getClash5ChainWorkspace()
      .then((next) => {
        if (!disposed)
          setLoadState({ profileId, workspace: next, failed: false })
      })
      .catch(() => {
        if (!disposed)
          setLoadState({ profileId, workspace: null, failed: true })
      })

    return () => {
      disposed = true
    }
  }, [profileId])

  const loading = loadState.profileId !== profileId
  const workspace = loading ? null : loadState.workspace
  const failed = !loading && loadState.failed
  const chains = workspace?.chains ?? []

  return (
    <EnhancedCard
      title={t('home.page.cards.chains')}
      icon={<LanOutlined />}
      iconColor="info"
      action={
        <Button size="small" onClick={() => navigate('/?view=chains')}>
          {t('home.page.cards.manageChains')}
        </Button>
      }
    >
      {failed ? (
        <Typography variant="body2" color="text.secondary">
          {t('home.page.cards.chainUnavailable')}
        </Typography>
      ) : loading ? (
        <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
          <CircularProgress size={18} />
          <Typography variant="body2" color="text.secondary">
            {t('home.page.cards.chainLoading')}
          </Typography>
        </Stack>
      ) : chains.length === 0 ? (
        <Typography variant="body2" color="text.secondary">
          {t('home.page.cards.chainEmpty')}
        </Typography>
      ) : (
        <Stack spacing={1}>
          {chains.slice(0, 3).map((chain) => (
            <Box
              key={chain.id}
              sx={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                gap: 1,
              }}
            >
              <Typography variant="body2" noWrap title={chain.name}>
                {chain.name || t('chains.name')}
              </Typography>
              <Chip
                size="small"
                color={chain.enabled ? 'success' : 'default'}
                label={t(chain.enabled ? 'chains.enabled' : 'chains.disabled')}
              />
            </Box>
          ))}
        </Stack>
      )}
    </EnhancedCard>
  )
}

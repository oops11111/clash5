import ExtensionOutlinedIcon from '@mui/icons-material/ExtensionOutlined'
import {
  Alert,
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  FormControl,
  MenuItem,
  Paper,
  Select,
  Stack,
  Tab,
  Tabs,
  TextField,
  Typography,
} from '@mui/material'
import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { BasePage } from '@/components/base'
import {
  bindClash5Node,
  listClash5Bindings,
  listClash5Nodes,
  getClash5IdentityStatus,
  listClash5PendingIdentityRebinds,
  resolveClash5IdentityRebind,
  rotateClash5Identity,
  resetClash5Identity,
  getClash5OrphanCleanupPreview,
  setClash5OrphanRetentionDays,
  purgeExpiredClash5Orphans,
  type Clash5PendingIdentityRebind,
  type Clash5Binding,
  type Clash5OrphanCleanupPreview,
} from '@/services/cmds'
import type { Clash5NodeRecord } from '@/types/clash5-node'

type PluginTab = 'installed' | 'market' | 'binding' | 'settings'

const tabs: PluginTab[] = ['installed', 'market', 'binding', 'settings']

const PluginsPage = () => {
  const { t } = useTranslation()
  const [tab, setTab] = useState<PluginTab>('installed')
  const [identityStatus, setIdentityStatus] = useState<{
    recovery_mode: boolean
    pending_rebinds: number
  } | null>(null)
  const [pendingRebinds, setPendingRebinds] = useState<
    Clash5PendingIdentityRebind[]
  >([])
  const [bindings, setBindings] = useState<Clash5Binding[]>([])
  const [nodes, setNodes] = useState<Clash5NodeRecord[]>([])
  const [selections, setSelections] = useState<Record<string, string>>({})
  const [resetOpen, setResetOpen] = useState(false)
  const [rotationOpen, setRotationOpen] = useState(false)
  const [identityError, setIdentityError] = useState(false)
  const [orphanCleanup, setOrphanCleanup] =
    useState<Clash5OrphanCleanupPreview | null>(null)
  const [retentionDraft, setRetentionDraft] = useState('30')
  const [cleanupOpen, setCleanupOpen] = useState(false)

  const refreshIdentity = useCallback(async () => {
    try {
      const [status, pending, savedBindings, savedNodes, cleanup] =
        await Promise.all([
          getClash5IdentityStatus(),
          listClash5PendingIdentityRebinds(),
          listClash5Bindings(),
          listClash5Nodes(),
          getClash5OrphanCleanupPreview(),
        ])
      setIdentityStatus(status)
      setPendingRebinds(pending)
      setBindings(savedBindings)
      setNodes(savedNodes)
      setOrphanCleanup(cleanup)
      setRetentionDraft(String(cleanup.retention_days))
      setIdentityError(false)
      return status
    } catch {
      setIdentityError(true)
      return null
    }
  }, [])

  useEffect(() => {
    if (tab === 'binding') void refreshIdentity()
  }, [refreshIdentity, tab])

  const confirmReset = async () => {
    try {
      await resetClash5Identity()
      setResetOpen(false)
      await refreshIdentity()
    } catch {
      setIdentityError(true)
      const status = await refreshIdentity()
      if (status && !status.recovery_mode) setResetOpen(false)
    }
  }

  const confirmRotation = async () => {
    try {
      await rotateClash5Identity()
      setRotationOpen(false)
      await refreshIdentity()
    } catch {
      setIdentityError(true)
    }
  }

  const saveOrphanRetention = async () => {
    const days = Number(retentionDraft)
    if (!Number.isInteger(days) || days < 1 || days > 365) return
    try {
      await setClash5OrphanRetentionDays(days)
      await refreshIdentity()
    } catch {
      setIdentityError(true)
    }
  }

  const confirmExpiredPurge = async () => {
    try {
      await purgeExpiredClash5Orphans()
      setCleanupOpen(false)
      await refreshIdentity()
    } catch {
      setIdentityError(true)
    }
  }

  const chooseRebind = async (item: Clash5PendingIdentityRebind) => {
    const candidate = selections[item.old_stable_id]
    if (!candidate) return
    try {
      await resolveClash5IdentityRebind(
        item.source,
        item.old_stable_id,
        candidate,
      )
      await refreshIdentity()
    } catch {
      setIdentityError(true)
    }
  }

  const chooseOrphanTarget = async (binding: Clash5Binding) => {
    const targetId = selections[`binding:${binding.id}`]
    if (!targetId) return
    try {
      const target = nodes.find(
        (node) => node.stable_id === targetId && node.active,
      )
      if (!target) return
      await bindClash5Node({
        ...binding,
        target_id: targetId,
        source: target.source,
      })
      await refreshIdentity()
    } catch {
      setIdentityError(true)
    }
  }

  const sourceMatches = (binding: Clash5Binding, node: Clash5NodeRecord) => {
    if (binding.source.kind === 'manual') return node.source.kind === 'manual'
    return (
      node.source.kind === 'subscription' &&
      binding.source.provider_id === node.source.provider_id
    )
  }

  return (
    <BasePage title={t('layout.components.navigation.tabs.plugins')}>
      <Stack spacing={2}>
        <Tabs
          value={tab}
          onChange={(_, value: PluginTab) => setTab(value)}
          variant="scrollable"
          scrollButtons="auto"
          aria-label={t('layout.components.navigation.tabs.plugins')}
        >
          {tabs.map((item) => (
            <Tab
              key={item}
              value={item}
              label={t(`layout.components.navigation.pluginTabs.${item}`)}
            />
          ))}
        </Tabs>
        {tab === 'binding' ? (
          <Stack spacing={2}>
            {identityError && (
              <Alert severity="error">
                {t('layout.components.navigation.identityError')}
              </Alert>
            )}
            {orphanCleanup && (
              <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
                <Stack spacing={1.5}>
                  <Alert
                    severity={
                      orphanCleanup.eligible_orphans > 0 ? 'warning' : 'info'
                    }
                    action={
                      orphanCleanup.eligible_orphans > 0 ? (
                        <Button
                          color="inherit"
                          disabled={identityStatus?.recovery_mode}
                          onClick={() => setCleanupOpen(true)}
                        >
                          {t(
                            'layout.components.navigation.reviewOrphanCleanup',
                          )}
                        </Button>
                      ) : undefined
                    }
                  >
                    {t('layout.components.navigation.orphanRetentionSummary', {
                      total: orphanCleanup.total_orphans,
                      eligible: orphanCleanup.eligible_orphans,
                      days: orphanCleanup.retention_days,
                    })}
                  </Alert>
                  <Stack
                    direction="row"
                    spacing={1}
                    sx={{ alignItems: 'center' }}
                  >
                    <TextField
                      label={t(
                        'layout.components.navigation.orphanRetentionDays',
                      )}
                      type="number"
                      size="small"
                      value={retentionDraft}
                      slotProps={{ htmlInput: { min: 1, max: 365 } }}
                      onChange={(event) =>
                        setRetentionDraft(event.target.value)
                      }
                      sx={{ maxWidth: 240 }}
                    />
                    <Button
                      variant="outlined"
                      disabled={
                        identityStatus?.recovery_mode ||
                        !Number.isInteger(Number(retentionDraft)) ||
                        Number(retentionDraft) < 1 ||
                        Number(retentionDraft) > 365 ||
                        Number(retentionDraft) === orphanCleanup.retention_days
                      }
                      onClick={() => void saveOrphanRetention()}
                    >
                      {t('layout.components.navigation.saveOrphanRetention')}
                    </Button>
                  </Stack>
                </Stack>
              </Paper>
            )}
            {identityStatus?.recovery_mode && (
              <Alert
                severity="warning"
                action={
                  <Button color="inherit" onClick={() => setResetOpen(true)}>
                    {t('layout.components.navigation.resetIdentity')}
                  </Button>
                }
              >
                {t('layout.components.navigation.identityRecovery')}
              </Alert>
            )}
            {identityStatus && !identityStatus.recovery_mode && (
              <Alert
                severity="info"
                action={
                  <Button color="inherit" onClick={() => setRotationOpen(true)}>
                    {t('layout.components.navigation.rotateIdentity')}
                  </Button>
                }
              >
                {t('layout.components.navigation.rotateIdentityDescription')}
              </Alert>
            )}
            {pendingRebinds.map((item) => (
              <Paper
                key={`${item.source.kind === 'subscription' ? item.source.provider_id : 'manual'}:${item.old_stable_id}`}
                variant="outlined"
                sx={{ p: 2, borderRadius: 2 }}
              >
                <Stack spacing={1.5}>
                  <Typography>
                    {t('layout.components.navigation.pendingRebind', {
                      id: item.old_stable_id,
                    })}
                  </Typography>
                  <Stack direction="row" spacing={1}>
                    <FormControl fullWidth size="small">
                      <Select<string>
                        value={selections[item.old_stable_id] ?? ''}
                        displayEmpty
                        onChange={(event) =>
                          setSelections((current) => ({
                            ...current,
                            [item.old_stable_id]: event.target.value,
                          }))
                        }
                      >
                        <MenuItem value="" disabled>
                          {t('layout.components.navigation.chooseCandidate')}
                        </MenuItem>
                        {item.candidate_ids.map((candidate) => (
                          <MenuItem key={candidate} value={candidate}>
                            {candidate}
                          </MenuItem>
                        ))}
                      </Select>
                    </FormControl>
                    <Button
                      variant="contained"
                      disabled={!selections[item.old_stable_id]}
                      onClick={() => void chooseRebind(item)}
                    >
                      {t('layout.components.navigation.rebind')}
                    </Button>
                  </Stack>
                </Stack>
              </Paper>
            ))}
            {bindings
              .filter(
                (binding) =>
                  binding.orphan &&
                  !pendingRebinds.some(
                    (item) =>
                      binding.source.kind === 'subscription' &&
                      item.old_stable_id === binding.target_id &&
                      item.source.kind === 'subscription' &&
                      item.source.provider_id === binding.source.provider_id,
                  ),
              )
              .map((binding) => {
                const key = `binding:${binding.id}`
                const candidates = nodes.filter(
                  (node) =>
                    node.active &&
                    node.stable_id.startsWith('v2:') &&
                    sourceMatches(binding, node),
                )
                return (
                  <Paper
                    key={key}
                    variant="outlined"
                    sx={{ p: 2, borderRadius: 2 }}
                  >
                    <Stack spacing={1.5}>
                      <Typography>
                        {t('layout.components.navigation.orphanBinding', {
                          id: binding.id,
                          target: binding.target_id,
                        })}
                      </Typography>
                      <Stack direction="row" spacing={1}>
                        <FormControl fullWidth size="small">
                          <Select<string>
                            value={selections[key] ?? ''}
                            displayEmpty
                            onChange={(event) =>
                              setSelections((current) => ({
                                ...current,
                                [key]: event.target.value,
                              }))
                            }
                          >
                            <MenuItem value="" disabled>
                              {t(
                                'layout.components.navigation.chooseCandidate',
                              )}
                            </MenuItem>
                            {candidates.map((node) => (
                              <MenuItem
                                key={node.stable_id}
                                value={node.stable_id}
                              >
                                {node.display_name}
                              </MenuItem>
                            ))}
                          </Select>
                        </FormControl>
                        <Button
                          variant="contained"
                          disabled={!selections[key]}
                          onClick={() => void chooseOrphanTarget(binding)}
                        >
                          {t('layout.components.navigation.rebind')}
                        </Button>
                      </Stack>
                    </Stack>
                  </Paper>
                )
              })}
            {pendingRebinds.length === 0 &&
              !bindings.some((binding) => binding.orphan) &&
              !identityStatus?.recovery_mode && (
                <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
                  <Stack
                    spacing={1}
                    sx={{ alignItems: 'center', textAlign: 'center' }}
                  >
                    <ExtensionOutlinedIcon color="primary" fontSize="large" />
                    <Typography variant="h6">
                      {t('layout.components.navigation.empty.bindingTitle')}
                    </Typography>
                    <Typography color="text.secondary">
                      {t(
                        'layout.components.navigation.empty.bindingDescription',
                      )}
                    </Typography>
                  </Stack>
                </Paper>
              )}
          </Stack>
        ) : (
          <Paper variant="outlined" sx={{ p: 4, borderRadius: 3 }}>
            <Stack
              spacing={1}
              sx={{ alignItems: 'center', textAlign: 'center' }}
            >
              <ExtensionOutlinedIcon color="primary" fontSize="large" />
              <Typography variant="h6">
                {t(`layout.components.navigation.empty.${tab}Title`)}
              </Typography>
              <Typography color="text.secondary">
                {t(`layout.components.navigation.empty.${tab}Description`)}
              </Typography>
            </Stack>
          </Paper>
        )}
      </Stack>
      <Dialog open={resetOpen} onClose={() => setResetOpen(false)}>
        <DialogTitle>
          {t('layout.components.navigation.resetIdentityTitle')}
        </DialogTitle>
        <DialogContent>
          <Typography>
            {t('layout.components.navigation.resetIdentityConfirm')}
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setResetOpen(false)}>
            {t('layout.components.navigation.cancel')}
          </Button>
          <Button
            color="warning"
            variant="contained"
            onClick={() => void confirmReset()}
          >
            {t('layout.components.navigation.resetIdentity')}
          </Button>
        </DialogActions>
      </Dialog>
      <Dialog open={rotationOpen} onClose={() => setRotationOpen(false)}>
        <DialogTitle>
          {t('layout.components.navigation.rotateIdentityTitle')}
        </DialogTitle>
        <DialogContent>
          <Typography>
            {t('layout.components.navigation.rotateIdentityConfirm')}
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setRotationOpen(false)}>
            {t('layout.components.navigation.cancel')}
          </Button>
          <Button
            color="warning"
            variant="contained"
            onClick={() => void confirmRotation()}
          >
            {t('layout.components.navigation.rotateIdentity')}
          </Button>
        </DialogActions>
      </Dialog>
      <Dialog open={cleanupOpen} onClose={() => setCleanupOpen(false)}>
        <DialogTitle>
          {t('layout.components.navigation.purgeExpiredOrphansTitle')}
        </DialogTitle>
        <DialogContent>
          <Typography>
            {t('layout.components.navigation.purgeExpiredOrphansConfirm', {
              count: orphanCleanup?.eligible_orphans ?? 0,
              days: orphanCleanup?.retention_days ?? 30,
            })}
          </Typography>
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setCleanupOpen(false)}>
            {t('layout.components.navigation.cancel')}
          </Button>
          <Button
            color="warning"
            variant="contained"
            disabled={
              !orphanCleanup?.eligible_orphans || identityStatus?.recovery_mode
            }
            onClick={() => void confirmExpiredPurge()}
          >
            {t('layout.components.navigation.purgeExpiredOrphans')}
          </Button>
        </DialogActions>
      </Dialog>
    </BasePage>
  )
}

export default PluginsPage

import { MenuItem, Select, Stack, Typography } from '@mui/material'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { saveClash5NodePool } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import type {
  NodePool,
  NodeRef,
  PoolMode,
  SwitchEvent,
} from '@/services/tauri-commands.generated'

const MODES: PoolMode[] = ['delay', 'pinned', 'quality']

interface Props {
  pool: NodePool
  nodes: { reference: NodeRef; display_name: string }[]
  lastSwitch?: SwitchEvent
  busy: boolean
  onSaved: () => Promise<void>
}

// Mode and member choice for one node pool. Changes apply when a line using the pool is activated again.
export const PoolPolicy = ({
  pool,
  nodes,
  lastSwitch,
  busy,
  onSaved,
}: Props) => {
  const { t } = useTranslation()
  const [saving, setSaving] = useState(false)
  const names = new Map(
    nodes.map((node) => [JSON.stringify(node.reference), node.display_name]),
  )
  const nameOf = (reference: NodeRef) =>
    names.get(JSON.stringify(reference)) ?? '—'
  const mode = pool.mode ?? 'delay'
  const reasonLabel = (reason: string) =>
    reason === 'failure'
      ? t('chains.poolPolicy.reasons.failure')
      : reason === 'score'
        ? t('chains.poolPolicy.reasons.score')
        : t('chains.poolPolicy.reasons.initial')
  const pinnedKey = pool.pinned ? JSON.stringify(pool.pinned) : ''

  const save = async (patch: Partial<Omit<NodePool, 'schema_version'>>) => {
    setSaving(true)
    try {
      await saveClash5NodePool({
        ...pool,
        schema_version: 2 as const,
        ...patch,
      })
      await onSaved()
    } catch (err) {
      showNotice.error(err)
    } finally {
      setSaving(false)
    }
  }

  return (
    <Stack spacing={1} sx={{ mt: 1.5 }}>
      <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
        <Typography variant="body2" sx={{ minWidth: 72 }}>
          {t('chains.poolPolicy.mode')}
        </Typography>
        <Select
          size="small"
          value={mode}
          disabled={busy || saving}
          sx={{ minWidth: 180 }}
          onChange={(event) =>
            void save({ mode: event.target.value as PoolMode })
          }
        >
          {MODES.map((value) => (
            <MenuItem key={value} value={value}>
              {t(`chains.poolPolicy.modes.${value}`)}
            </MenuItem>
          ))}
        </Select>
      </Stack>
      {mode !== 'delay' && (
        <Stack direction="row" spacing={1} sx={{ alignItems: 'center' }}>
          <Typography variant="body2" sx={{ minWidth: 72 }}>
            {t('chains.poolPolicy.member')}
          </Typography>
          <Select
            size="small"
            value={pinnedKey}
            disabled={busy || saving}
            sx={{ minWidth: 180 }}
            onChange={(event) => {
              const member = pool.members.find(
                (reference) => JSON.stringify(reference) === event.target.value,
              )
              if (member) void save({ pinned: member })
            }}
          >
            {pool.members.map((member) => (
              <MenuItem
                key={JSON.stringify(member)}
                value={JSON.stringify(member)}
              >
                {nameOf(member)}
              </MenuItem>
            ))}
          </Select>
        </Stack>
      )}
      <Typography variant="caption" color="text.secondary">
        {t('chains.poolPolicy.applyHint')}
      </Typography>
      {mode === 'quality' && lastSwitch && (
        <Typography variant="caption" color="text.secondary">
          {t('chains.poolPolicy.lastSwitch', {
            member: lastSwitch.member,
            reason: reasonLabel(lastSwitch.reason),
          })}
        </Typography>
      )}
    </Stack>
  )
}

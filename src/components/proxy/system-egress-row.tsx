import { Box, MenuItem, Select, Stack, Typography } from '@mui/material'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useProxySelection } from '@/hooks/use-proxy-selection'
import { useProxiesData } from '@/providers/app-data-context'
import { getClash5RuleOverview } from '@/services/cmds'
import {
  type InteractableProxyMember,
  isInteractableMember,
  resolveMember,
} from '@/types/proxy-view'

const memberName = (member: InteractableProxyMember) =>
  member.kind === 'node' ? member.node.name : member.group.name

// Traffic that no program route claims leaves through the default group; this row picks its node.
export const SystemEgressRow = () => {
  const { t } = useTranslation()
  const { proxyView } = useProxiesData()
  const [defaultGroup, setDefaultGroup] = useState<string | null>(null)
  const { handleProxyGroupChange } = useProxySelection()

  useEffect(() => {
    getClash5RuleOverview()
      .then((overview) => setDefaultGroup(overview.default_group))
      .catch(() => setDefaultGroup(null))
  }, [proxyView])

  const group =
    proxyView && defaultGroup
      ? (proxyView.groups.find(({ name }) => name === defaultGroup) ??
        (proxyView.global?.name === defaultGroup
          ? proxyView.global
          : undefined))
      : undefined
  if (!proxyView || !group) return null

  const options = group.members
    .map((member) => resolveMember(proxyView, member))
    .filter(isInteractableMember)

  return (
    <Stack
      direction="row"
      spacing={2}
      sx={{
        alignItems: 'center',
        justifyContent: 'space-between',
        px: 2,
        py: 1,
      }}
    >
      <Box sx={{ minWidth: 0 }}>
        <Typography sx={{ fontWeight: 600 }}>
          {t('proxies.systemEgress.title')}
        </Typography>
        <Typography variant="caption" color="text.secondary">
          {t('proxies.systemEgress.hint', { group: group.name })}
        </Typography>
      </Box>
      <Select
        size="small"
        value={group.now ?? ''}
        sx={{ minWidth: 200 }}
        onChange={(event) => {
          const member = options.find(
            (option) => memberName(option) === event.target.value,
          )
          if (member)
            handleProxyGroupChange(group, { name: memberName(member) })
        }}
      >
        {options.map((option) => (
          <MenuItem key={memberName(option)} value={memberName(option)}>
            {memberName(option)}
          </MenuItem>
        ))}
      </Select>
    </Stack>
  )
}

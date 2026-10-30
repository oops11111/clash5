import { styled, Box, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { Rule } from 'tauri-plugin-mihomo-api'

import type { RuleNote } from '@/services/tauri-commands.generated'

const Item = styled(Box)(({ theme }) => ({
  display: 'flex',
  padding: '4px 16px',
  color: theme.palette.text.primary,
}))

const COLOR = [
  'primary',
  'secondary',
  'info.main',
  'warning.main',
  'success.main',
]

interface Props {
  value: Rule & { lineNo: number }
  note?: RuleNote
}

const parseColor = (text: string) => {
  if (text === 'REJECT' || text === 'REJECT-DROP') return 'error.main'
  if (text === 'DIRECT') return 'text.primary'

  let sum = 0
  for (let i = 0; i < text.length; i++) {
    sum += text.charCodeAt(i)
  }
  return COLOR[sum % COLOR.length]
}

const RuleItem = (props: Props) => {
  const { value, note } = props
  const { t } = useTranslation()

  return (
    <Item sx={{ borderBottom: '1px solid var(--divider-color)' }}>
      <Typography
        color="text.secondary"
        variant="body2"
        sx={{ lineHeight: 2, minWidth: 30, mr: 2.25, textAlign: 'center' }}
      >
        {value.lineNo}
      </Typography>

      <Box sx={{ userSelect: 'text' }}>
        <Typography component="h6" variant="subtitle1" color="text.primary">
          {value.payload || '-'}
        </Typography>

        <Typography
          component="span"
          variant="body2"
          color="text.secondary"
          sx={{ mr: 3, minWidth: 120, display: 'inline-block' }}
        >
          {typeof value.type === 'string' ? value.type : value.type.Unknown}
        </Typography>

        <Typography
          component="span"
          variant="body2"
          color={parseColor(value.proxy)}
        >
          {value.proxy}
        </Typography>

        {note && (
          <Typography
            component="div"
            variant="caption"
            color="text.secondary"
            sx={{ mt: 0.25 }}
          >
            {note.source ?? t('rules.notes.subscription')}
            {note.merged > 0 &&
              ` · ${t('rules.notes.merged', { count: note.merged })}`}
          </Typography>
        )}
      </Box>
    </Item>
  )
}

export default RuleItem

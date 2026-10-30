import { DeleteOutlineRounded } from '@mui/icons-material'
import {
  Box,
  Button,
  IconButton,
  Stack,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from '@mui/material'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { useVerge } from '@/hooks/use-verge'
import { showNotice } from '@/services/notice-service'
import {
  parseTextRules,
  previewTextRules,
  type TextRuleEntry,
} from '@/utils/text-rules'

type RuleTargetKey = 'default' | 'direct' | 'reject'

const TARGETS: RuleTargetKey[] = ['default', 'direct', 'reject']

interface Props {
  rules: { type: unknown; payload?: string }[]
}

const ruleKind = (type: unknown): string => {
  if (typeof type === 'string') return type.toUpperCase()
  if (type && typeof type === 'object' && 'Unknown' in type) {
    return String((type as { Unknown: unknown }).Unknown).toUpperCase()
  }
  return ''
}

export const TextRuleSets = ({ rules }: Props) => {
  const { t } = useTranslation()
  const { verge, patchVerge } = useVerge()
  const sets = verge?.text_rule_sets ?? []

  const [name, setName] = useState('')
  const [content, setContent] = useState('')
  const [target, setTarget] = useState<RuleTargetKey>('reject')

  const existing = useMemo<TextRuleEntry[]>(
    () =>
      rules
        .filter((rule) => rule.payload)
        .map((rule) => ({
          kind: ruleKind(rule.type),
          payload: rule.payload ?? '',
        })),
    [rules],
  )

  const parsed = useMemo(() => parseTextRules(content), [content])
  const preview = useMemo(
    () => previewTextRules(parsed, existing),
    [parsed, existing],
  )

  const canAdd = name.trim() !== '' && preview.valid > 0

  const onAdd = async () => {
    try {
      await patchVerge({
        text_rule_sets: [
          ...sets,
          {
            id: crypto.randomUUID(),
            name: name.trim(),
            target,
            content,
          },
        ],
      })
      setName('')
      setContent('')
    } catch (err) {
      showNotice.error(err)
    }
  }

  const onDelete = async (id: string) => {
    try {
      await patchVerge({
        text_rule_sets: sets.filter((set) => set.id !== id),
      })
    } catch (err) {
      showNotice.error(err)
    }
  }

  return (
    <Box>
      <Typography variant="body2" color="text.secondary" sx={{ mb: 1.5 }}>
        {t('rules.textRules.hint')}
      </Typography>

      {sets.length === 0 ? (
        <Typography variant="body2" color="text.secondary">
          {t('rules.textRules.empty')}
        </Typography>
      ) : (
        <Stack spacing={0.5} sx={{ mb: 2 }}>
          {sets.map((set) => (
            <Stack
              key={set.id}
              direction="row"
              spacing={1}
              sx={{ alignItems: 'center', justifyContent: 'space-between' }}
            >
              <Box sx={{ minWidth: 0 }}>
                <Typography noWrap sx={{ fontWeight: 600 }}>
                  {set.name}
                </Typography>
                <Typography variant="caption" color="text.secondary">
                  {t(`rules.textRules.targets.${set.target}`)} ·{' '}
                  {t('rules.textRules.count', {
                    count: parseTextRules(set.content).entries.length,
                  })}
                </Typography>
              </Box>
              <IconButton
                size="small"
                aria-label={t('rules.textRules.delete', { name: set.name })}
                onClick={() => void onDelete(set.id)}
              >
                <DeleteOutlineRounded fontSize="small" />
              </IconButton>
            </Stack>
          ))}
        </Stack>
      )}

      <Stack spacing={1.5}>
        <TextField
          size="small"
          label={t('rules.textRules.name')}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <TextField
          multiline
          minRows={4}
          label={t('rules.textRules.content')}
          placeholder={t('rules.textRules.placeholder')}
          value={content}
          onChange={(e) => setContent(e.target.value)}
        />
        <Stack
          direction="row"
          spacing={1}
          sx={{ alignItems: 'center', justifyContent: 'space-between' }}
        >
          <ToggleButtonGroup
            exclusive
            size="small"
            aria-label={t('rules.textRules.targetLabel')}
            value={target}
            onChange={(_, value: RuleTargetKey | null) => {
              if (value) setTarget(value)
            }}
          >
            {TARGETS.map((key) => (
              <ToggleButton key={key} value={key}>
                {t(`rules.textRules.targets.${key}`)}
              </ToggleButton>
            ))}
          </ToggleButtonGroup>
          <Button variant="contained" disabled={!canAdd} onClick={onAdd}>
            {t('rules.textRules.add')}
          </Button>
        </Stack>
        {content.trim() !== '' && (
          <Typography variant="body2" color="text.secondary" role="status">
            {t('rules.textRules.preview', {
              valid: preview.valid,
              added: preview.added,
              merged: preview.merged,
              invalid: preview.invalid,
            })}
          </Typography>
        )}
      </Stack>
    </Box>
  )
}

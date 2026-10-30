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
import { isValidUrl } from '@/utils/network'
import {
  parseTextRules,
  previewTextRules,
  ruleKind,
  type TextRuleEntry,
} from '@/utils/text-rules'

type RuleTargetKey = 'default' | 'direct' | 'reject'

const hostOf = (url: string) => {
  try {
    return new URL(url).host
  } catch {
    return url
  }
}

const TARGETS: RuleTargetKey[] = ['default', 'direct', 'reject']

type Mode = 'paste' | 'link'

interface Props {
  rules: { type: unknown; payload?: string }[]
}

export const TextRuleSets = ({ rules }: Props) => {
  const { t } = useTranslation()
  const { verge, patchVerge } = useVerge()
  const sets = verge?.text_rule_sets ?? []

  const [mode, setMode] = useState<Mode>('paste')
  const [name, setName] = useState('')
  const [content, setContent] = useState('')
  const [url, setUrl] = useState('')
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

  const linkValid = isValidUrl(url.trim())
  const canAdd =
    name.trim() !== '' && (mode === 'link' ? linkValid : preview.valid > 0)

  const onAdd = async () => {
    try {
      await patchVerge({
        text_rule_sets: [
          ...sets,
          {
            id: crypto.randomUUID(),
            name: name.trim(),
            target,
            content: mode === 'paste' ? content : '',
            url: mode === 'link' ? url.trim() : null,
          },
        ],
      })
      setName('')
      setContent('')
      setUrl('')
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
                  {set.url
                    ? t('rules.textRules.linked', { host: hostOf(set.url) })
                    : t('rules.textRules.count', {
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
        <ToggleButtonGroup
          exclusive
          size="small"
          aria-label={t('rules.textRules.modeLabel')}
          value={mode}
          onChange={(_, value: Mode | null) => {
            if (value) setMode(value)
          }}
        >
          <ToggleButton value="paste">
            {t('rules.textRules.modes.paste')}
          </ToggleButton>
          <ToggleButton value="link">
            {t('rules.textRules.modes.link')}
          </ToggleButton>
        </ToggleButtonGroup>
        <TextField
          size="small"
          label={t('rules.textRules.name')}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        {mode === 'paste' ? (
          <TextField
            multiline
            minRows={4}
            label={t('rules.textRules.content')}
            placeholder={t('rules.textRules.placeholder')}
            value={content}
            onChange={(e) => setContent(e.target.value)}
          />
        ) : (
          <>
            <TextField
              size="small"
              label={t('rules.textRules.url')}
              placeholder="https://"
              value={url}
              error={url.trim() !== '' && !linkValid}
              onChange={(e) => setUrl(e.target.value)}
            />
            <Typography variant="caption" color="text.secondary">
              {t('rules.textRules.linkHint')}
            </Typography>
          </>
        )}
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
        {mode === 'paste' && content.trim() !== '' && (
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

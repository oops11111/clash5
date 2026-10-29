import { Button, Paper, Stack, TextField, Typography } from '@mui/material'
import { useLockFn } from 'ahooks'
import { useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { importClash5ManualProxies } from '@/services/cmds'
import { showNotice } from '@/services/notice-service'
import parseUri from '@/utils/uri-parser'

// One share link per line. Lines that do not parse, or have no endpoint, are reported and not saved.
const parseLines = (text: string) => {
  const proxies: IProxyConfig[] = []
  const errorLines: number[] = []
  text.split(/\r?\n/).forEach((raw, index) => {
    const line = raw.trim()
    if (!line) return
    try {
      const proxy = parseUri(line)
      if (proxy.server && proxy.port) proxies.push(proxy)
      else errorLines.push(index + 1)
    } catch {
      errorLines.push(index + 1)
    }
  })
  return { proxies, errorLines }
}

// Manual nodes are not subscriptions: they have no source to refresh and stay separate from them.
export const ManualNodesCard = () => {
  const { t } = useTranslation()
  const [text, setText] = useState('')
  const preview = useMemo(() => parseLines(text), [text])

  const save = useLockFn(async () => {
    if (preview.proxies.length === 0) return
    try {
      const result = await importClash5ManualProxies(preview.proxies)
      setText('')
      showNotice.success(
        t('nodePool.manualImportSuccess', { count: result.imported }),
      )
    } catch (error) {
      showNotice.error(error)
    }
  })

  return (
    <Paper variant="outlined" sx={{ p: 2, borderRadius: 3, m: 2 }}>
      <Stack spacing={1.5}>
        <Typography variant="subtitle1">
          {t('profiles.manualNodes.title')}
        </Typography>
        <Typography variant="body2" color="text.secondary">
          {t('profiles.manualNodes.hint')}
        </Typography>
        <TextField
          multiline
          minRows={3}
          size="small"
          value={text}
          onChange={(event) => setText(event.target.value)}
          placeholder={t('profiles.manualNodes.placeholder')}
          slotProps={{
            htmlInput: { 'aria-label': t('profiles.manualNodes.title') },
          }}
        />
        {text.trim() !== '' && (
          <Typography variant="body2" color="text.secondary">
            {t('profiles.manualNodes.preview', {
              count: preview.proxies.length,
              errors: preview.errorLines.length,
            })}
          </Typography>
        )}
        <Stack direction="row" justifyContent="flex-end">
          <Button
            variant="contained"
            size="small"
            disabled={preview.proxies.length === 0}
            onClick={() => void save()}
          >
            {t('profiles.manualNodes.save')}
          </Button>
        </Stack>
      </Stack>
    </Paper>
  )
}

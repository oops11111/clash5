import { List, ListItem } from '@mui/material'

export const SkippedImportReasons = ({ reasons }: { reasons: string[] }) => {
  const occurrences = new Map<string, number>()

  return (
    <List dense disablePadding>
      {reasons.map((reason) => {
        const occurrence = occurrences.get(reason) ?? 0
        occurrences.set(reason, occurrence + 1)

        return (
          <ListItem
            key={`${reason}-${occurrence}`}
            data-testid="skipped-import-reason"
            sx={{ px: 0, whiteSpace: 'pre-wrap' }}
          >
            {reason}
          </ListItem>
        )
      })}
    </List>
  )
}

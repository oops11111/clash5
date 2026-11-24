import { Alert } from '@mui/material'
import { useTranslation } from 'react-i18next'

import { boundNoticeText } from '@/services/notice-service'
import { useConnectionStatus } from '@/store/app-store-context'

/**
 * Persistent banner while the backend link is down. The store's connection
 * state is fed by WS subscriptions and backend queries, and reconnects
 * happen automatically, so the banner only informs, it never acts.
 */
export const ConnectionBanner = () => {
  const { t } = useTranslation()
  const connection = useConnectionStatus()
  const isDown =
    connection.websocket === 'reconnecting' ||
    connection.backend === 'reconnecting'
  if (!isDown) return null

  return (
    <Alert severity="warning" sx={{ borderRadius: 0 }}>
      {t('shared.feedback.errors.backendDisconnected')}
      {connection.message ? ` — ${boundNoticeText(connection.message)}` : null}
    </Alert>
  )
}

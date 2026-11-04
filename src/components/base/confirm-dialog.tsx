import { Alert, Stack } from '@mui/material'
import type { ReactNode } from 'react'

import { BaseDialog } from './base-dialog'

interface Props {
  open: boolean
  title: ReactNode
  message: ReactNode
  confirmLabel: ReactNode
  cancelLabel: ReactNode
  warning?: ReactNode
  loading?: boolean
  onConfirm: () => void
  onCancel: () => void
}

export const ConfirmDialog: React.FC<Props> = ({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  warning,
  loading,
  onConfirm,
  onCancel,
}) => (
  <BaseDialog
    open={open}
    title={title}
    okBtn={confirmLabel}
    cancelBtn={cancelLabel}
    loading={loading}
    onOk={onConfirm}
    onCancel={onCancel}
    onClose={onCancel}
  >
    <Stack spacing={2}>
      {warning && <Alert severity="warning">{warning}</Alert>}
      {message}
    </Stack>
  </BaseDialog>
)

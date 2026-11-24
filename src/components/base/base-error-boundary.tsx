import { Button, Stack } from '@mui/material'
import { ReactNode } from 'react'
import { ErrorBoundary, FallbackProps } from 'react-error-boundary'
import { useTranslation } from 'react-i18next'

function ErrorFallback({
  error,
  resetErrorBoundary,
  onGoHome,
}: FallbackProps & { onGoHome?: () => void }) {
  const { t } = useTranslation()
  const errorMessage = error instanceof Error ? error.message : String(error)
  const errorStack = error instanceof Error ? error.stack : undefined

  return (
    <div role="alert" style={{ padding: 16 }}>
      <h4>{t('shared.feedback.errors.unexpected')}</h4>

      <pre>{errorMessage}</pre>

      <Stack direction="row" spacing={1} sx={{ marginBottom: 2 }}>
        <Button variant="contained" onClick={resetErrorBoundary}>
          {t('shared.actions.retry')}
        </Button>
        {onGoHome && (
          <Button
            variant="outlined"
            onClick={() => {
              onGoHome()
              resetErrorBoundary()
            }}
          >
            {t('layout.components.navigation.tabs.home')}
          </Button>
        )}
      </Stack>

      <details title={t('shared.feedback.errors.stack')}>
        <summary>{t('shared.feedback.errors.stack')}</summary>
        <pre>{errorStack}</pre>
      </details>
    </div>
  )
}

interface Props {
  children?: ReactNode
  /** Reset the boundary when any key changes (e.g. navigating away). */
  resetKeys?: unknown[]
  onReset?: () => void
  /** Escape hatch back to the home page; the button is hidden without it. */
  onGoHome?: () => void
}

export const BaseErrorBoundary = ({
  children,
  resetKeys,
  onReset,
  onGoHome,
}: Props) => {
  return (
    <ErrorBoundary
      FallbackComponent={(props) => (
        <ErrorFallback {...props} onGoHome={onGoHome} />
      )}
      resetKeys={resetKeys}
      onReset={onReset}
    >
      {children}
    </ErrorBoundary>
  )
}

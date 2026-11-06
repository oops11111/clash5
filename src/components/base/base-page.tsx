import { Typography } from '@mui/material'
import { useTheme } from '@mui/material/styles'
import type React from 'react'
import type { ReactNode } from 'react'

import { BaseErrorBoundary } from './base-error-boundary'

interface Props {
  title?: React.ReactNode // the page title
  header?: React.ReactNode // something behind title
  contentStyle?: React.CSSProperties
  children?: ReactNode
  full?: boolean
  embedded?: boolean
}

export const BasePage: React.FC<Props> = (props) => {
  const { title, header, contentStyle, full, embedded, children } = props
  const theme = useTheme()

  if (embedded) {
    return (
      <BaseErrorBoundary>
        <div className="base-embedded-page">
          {header && <div className="base-embedded-header">{header}</div>}
          <div className="base-embedded-content" style={contentStyle}>
            {children}
          </div>
        </div>
      </BaseErrorBoundary>
    )
  }

  return (
    <BaseErrorBoundary>
      <div className="base-page">
        <header data-tauri-drag-region="true" style={{ userSelect: 'none' }}>
          <Typography
            sx={{ fontSize: '20px', fontWeight: '700 ' }}
            data-tauri-drag-region="true"
          >
            {title}
          </Typography>

          {header}
        </header>

        <div
          className={full ? 'base-container no-padding' : 'base-container'}
          style={{ backgroundColor: theme.palette.background.default }}
        >
          <section
            style={{
              backgroundColor: theme.palette.background.default,
            }}
          >
            <div className="base-content" style={contentStyle}>
              {children}
            </div>
          </section>
        </div>
      </div>
    </BaseErrorBoundary>
  )
}

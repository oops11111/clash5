import { Typography } from '@mui/material'
import { useTheme } from '@mui/material/styles'
import React, { ReactNode } from 'react'
import { useLocation, useNavigate } from 'react-router'

import { BaseErrorBoundary } from './base-error-boundary'

interface Props {
  title?: React.ReactNode // the page title
  header?: React.ReactNode // something behind title
  contentStyle?: React.CSSProperties
  children?: ReactNode
  full?: boolean
  compact?: boolean
}

export const BasePage: React.FC<Props> = (props) => {
  const { title, header, contentStyle, full, compact, children } = props
  const theme = useTheme()
  const navigate = useNavigate()
  const { pathname } = useLocation()

  return (
    <BaseErrorBoundary resetKeys={[pathname]} onGoHome={() => navigate('/')}>
      <div className={`base-page${compact ? ' base-page--compact' : ''}`}>
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

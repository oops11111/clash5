import { ButtonBase } from '@mui/material'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useLocation, useNavigate } from 'react-router'

import brandIcon from '@/assets/image/clash5-icon.png'
import { primaryNavigationItems } from '@/pages/_navigation-meta'

import { UpdateButton } from './update-button'

interface LayoutTopNavProps {
  isDark: boolean
}

/** R14's five-tab shell. Legacy pages remain reachable through deep links. */
export const LayoutTopNav = ({ isDark }: LayoutTopNavProps) => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const { pathname } = useLocation()

  const tabs = useMemo(() => Object.values(primaryNavigationItems), [])

  return (
    <nav className="layout-top-nav" aria-label="Primary navigation">
      <div className="layout-top-nav__brand" data-tauri-drag-region="false">
        <img src={brandIcon} alt="clash5" />
        <span style={{ color: isDark ? '#fff' : '#111827' }}>clash5</span>
      </div>

      <div className="layout-top-nav__tabs" role="tablist">
        {tabs.map((tab) => {
          const selected =
            tab.path === '/'
              ? pathname === '/'
              : pathname === tab.path || pathname.startsWith(`${tab.path}/`)

          return (
            <ButtonBase
              key={tab.path}
              className={`layout-top-nav__tab${selected ? ' is-active' : ''}`}
              role="tab"
              aria-selected={selected}
              aria-current={selected ? 'page' : undefined}
              onClick={() => navigate(tab.path)}
            >
              {t(tab.label)}
            </ButtonBase>
          )
        })}
      </div>

      <div className="layout-top-nav__actions">
        <UpdateButton className="layout-top-nav__update" />
      </div>
    </nav>
  )
}

import { Box, Tab, Tabs } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useLocation, useNavigate } from 'react-router'

import brandIcon from '@/assets/image/clash5-icon.png'
import { navItems } from '@/pages/_navigation'

import { UpdateButton } from './update-button'

interface LayoutSidebarProps {
  isDark: boolean
}

export const LayoutSidebar = (props: LayoutSidebarProps) => {
  const { isDark } = props
  const { t } = useTranslation()
  const location = useLocation()
  const navigate = useNavigate()
  const selected =
    navItems.find((item) => item.path === location.pathname)?.path ?? '/'

  return (
    <div className="layout-content__navigation">
      <div className="the-logo" data-tauri-drag-region="false">
        <img
          src={brandIcon}
          alt="clash5"
          style={{ height: '30px', width: '30px' }}
        />
        <span
          style={{
            color: isDark ? 'white' : '#111827',
            fontSize: 17,
            fontWeight: 700,
            letterSpacing: '0.01em',
            lineHeight: '27px',
          }}
        >
          clash5
        </span>
      </div>
      <Tabs
        className="the-menu"
        value={selected}
        onChange={(_, value: string) => navigate(value)}
        aria-label={t('layout.components.navigation.tabs.home')}
        variant="scrollable"
        scrollButtons="auto"
      >
        {navItems.map((item) => (
          <Tab
            key={item.path}
            value={item.path}
            label={t(item.label)}
            data-testid={`main-tab-${item.path === '/' ? 'home' : item.path.slice(1)}`}
          />
        ))}
      </Tabs>
      <Box className="navigation-actions">
        <UpdateButton />
      </Box>
    </div>
  )
}

import {
  alpha,
  ListItem,
  ListItemButton,
  ListItemIcon,
  ListItemText,
} from '@mui/material'
import type { ReactNode } from 'react'
import { useMatch, useNavigate, useResolvedPath } from 'react-router'

import type { SortableItemRenderProps } from '@/components/base/sortable-item'
import { useVerge } from '@/hooks/use-verge'

interface Props {
  to: string
  children: string
  icon: ReactNode[]
  sortable?: SortableItemRenderProps
}
export const LayoutItem = (props: Props) => {
  const { to, children, icon, sortable } = props
  const { verge } = useVerge()
  const { menu_icon } = verge ?? {}
  const navCollapsed = verge?.collapse_navbar ?? false
  const resolved = useResolvedPath(to)
  const match = useMatch({ path: resolved.pathname, end: true })
  const navigate = useNavigate()

  const effectiveMenuIcon =
    navCollapsed && menu_icon === 'disable' ? 'monochrome' : menu_icon

  return (
    <ListItem
      ref={sortable?.ref}
      style={sortable?.style}
      sx={{ py: 0.125, maxWidth: 250, mx: 'auto' }}
    >
      <ListItemButton
        ref={sortable?.handleRef}
        selected={!!match}
        sx={[
          {
            borderRadius: '10px',
            marginLeft: 1.25,
            paddingLeft: 1,
            paddingRight: 1,
            marginRight: 1.25,
            cursor: 'pointer',
            '& .MuiListItemText-primary': {
              color: 'text.secondary',
              fontWeight: 600,
            },
          },
          ({ palette }) => ({
            '&:hover': { bgcolor: palette.action.hover },
            '&.Mui-selected': {
              bgcolor: palette.primary.main,
              boxShadow: `0 4px 12px ${alpha(palette.primary.main, 0.28)}`,
            },
            '&.Mui-selected:hover': { bgcolor: palette.primary.dark },
            '&.Mui-selected .MuiListItemText-primary': { color: '#fff' },
            '&.Mui-selected .MuiListItemIcon-root': { color: '#fff' },
          }),
        ]}
        title={navCollapsed ? children : undefined}
        aria-label={navCollapsed ? children : undefined}
        onClick={() => navigate(to)}
      >
        {(effectiveMenuIcon === 'monochrome' || !effectiveMenuIcon) && (
          <ListItemIcon
            sx={{
              color: 'text.secondary',
              minWidth: '44px',
              marginLeft: '6px',
              cursor: 'inherit',
            }}
          >
            {icon[0]}
          </ListItemIcon>
        )}
        {effectiveMenuIcon === 'colorful' && (
          <ListItemIcon
            sx={{ minWidth: '44px', marginLeft: '6px', cursor: 'inherit' }}
          >
            {icon[1]}
          </ListItemIcon>
        )}
        <ListItemText
          sx={{
            textAlign: effectiveMenuIcon === 'disable' ? 'center' : 'left',
          }}
          primary={children}
        />
      </ListItemButton>
    </ListItem>
  )
}

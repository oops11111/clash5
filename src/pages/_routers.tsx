import { Box, Button, Stack, Typography } from '@mui/material'
import { useTranslation } from 'react-i18next'
import {
  createBrowserRouter,
  Navigate,
  RouteObject,
  useNavigate,
  useRouteError,
} from 'react-router'

import Layout from './_layout'
import { navItems } from './_navigation'
import { NotFoundPage } from './not-found'

/**
 * Full-page fallback for errors that escape the layout's error boundary
 * (e.g. thrown while rendering the layout itself). Retry re-runs the
 * current route; Home navigates away from the failing route.
 */
const RouteErrorPage = () => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const error = useRouteError()
  const errorMessage = error instanceof Error ? error.message : String(error)

  return (
    <Box role="alert" sx={{ padding: 2 }}>
      <Typography variant="h6">
        {t('shared.feedback.errors.unexpected')}
      </Typography>
      <Typography variant="body2" sx={{ whiteSpace: 'pre-wrap' }}>
        {errorMessage}
      </Typography>
      <Stack direction="row" spacing={1} sx={{ marginTop: 2 }}>
        <Button variant="contained" onClick={() => navigate(0)}>
          {t('shared.actions.retry')}
        </Button>
        <Button variant="outlined" onClick={() => navigate('/')}>
          {t('layout.components.navigation.tabs.home')}
        </Button>
      </Stack>
    </Box>
  )
}

export const router = createBrowserRouter([
  {
    path: '/',
    Component: Layout,
    errorElement: <RouteErrorPage />,
    children: navItems
      .map(
        (item) =>
          ({
            path: item.path,
            Component: item.Component,
          }) as RouteObject,
      )
      .concat([
        {
          path: '/nodes',
          element: <Navigate replace to="/subscriptions?section=nodes" />,
        },
        {
          path: '/profile',
          element: <Navigate replace to="/subscriptions?section=sources" />,
        },
        {
          path: '/chains',
          element: <Navigate replace to="/?section=chains" />,
        },
        {
          path: '/rules',
          element: <Navigate replace to="/proxies?section=rules" />,
        },
        {
          path: '/connections',
          element: <Navigate replace to="/settings?section=connections" />,
        },
        {
          path: '/logs',
          element: <Navigate replace to="/settings?section=logs" />,
        },
        {
          path: '/unlock',
          element: <Navigate replace to="/settings?section=unlock" />,
        },
        {
          path: '*',
          Component: NotFoundPage,
        },
      ]),
  },
])

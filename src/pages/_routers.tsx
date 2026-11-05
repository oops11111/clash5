import { createBrowserRouter, Navigate, RouteObject } from 'react-router'

import Layout from './_layout'
import { navItems } from './_navigation'

export const router = createBrowserRouter([
  {
    path: '/',
    Component: Layout,
    children: navItems
      .map(
        (item) =>
          ({
            path: item.path,
            Component: item.Component,
          }) as RouteObject,
      )
      .concat([
        // Keep saved links working while R14 exposes only the five primary tabs.
        {
          path: '/nodes',
          element: <Navigate to="/profile?view=nodes" replace />,
        },
        { path: '/chains', element: <Navigate to="/?view=chains" replace /> },
        {
          path: '/rules',
          element: <Navigate to="/proxies?view=rules" replace />,
        },
        {
          path: '/diagnostics',
          element: <Navigate to="/settings?view=diagnostics" replace />,
        },
        {
          path: '/unlock',
          element: <Navigate to="/settings?view=unlock" replace />,
        },
        {
          path: '/connections',
          element: <Navigate to="/settings?view=connections" replace />,
        },
        {
          path: '/logs',
          element: <Navigate to="/settings?view=logs" replace />,
        },
      ]),
  },
])

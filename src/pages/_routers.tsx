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
      ]),
  },
])

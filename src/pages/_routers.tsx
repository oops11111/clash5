import { createBrowserRouter, RouteObject } from 'react-router'

import Layout from './_layout'
import { navItems } from './_navigation'
import ConnectionsPage from './connections'
import LogsPage from './logs'
import UnlockPage from './unlock'

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
        { path: '/unlock', Component: UnlockPage },
        // Still routed so a saved start page keeps working; the sidebar shows Diagnostics.
        { path: '/connections', Component: ConnectionsPage },
        { path: '/logs', Component: LogsPage },
      ]),
  },
])

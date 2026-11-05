import { createBrowserRouter, RouteObject } from 'react-router'

import Layout from './_layout'
import { navItems } from './_navigation'
import ChainsPage from './chains'
import ConnectionsPage from './connections'
import DiagnosticsPage from './diagnostics'
import LogsPage from './logs'
import NodePoolPage from './node-pool'
import RulesPage from './rules'
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
        // Legacy routes remain available for saved start pages and deep links while R14
        // folds them into the five primary tabs.
        { path: '/nodes', Component: NodePoolPage },
        { path: '/chains', Component: ChainsPage },
        { path: '/rules', Component: RulesPage },
        { path: '/diagnostics', Component: DiagnosticsPage },
        { path: '/unlock', Component: UnlockPage },
        { path: '/connections', Component: ConnectionsPage },
        { path: '/logs', Component: LogsPage },
      ]),
  },
])

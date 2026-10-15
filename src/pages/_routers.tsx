import { createBrowserRouter, RouteObject } from 'react-router'

import Layout from './_layout'
import { navItems } from './_navigation'
import ProxyPage from './proxies'
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
        { path: '/proxies', Component: ProxyPage },
        { path: '/unlock', Component: UnlockPage },
      ]),
  },
])

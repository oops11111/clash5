import type { ComponentType } from 'react'

import { navigationItems } from './_navigation-meta'
import HomeWorkspace from './home-workspace'
import PluginsPage from './plugins'
import ProxyWorkspace from './proxy-workspace'
import SettingsWorkspace from './settings-workspace'
import SubscriptionWorkspace from './subscription-workspace'

type NavigationItem = {
  label: string
  path: string
  Component: ComponentType
}

export const navItems: NavigationItem[] = [
  { ...navigationItems.home, Component: HomeWorkspace },
  { ...navigationItems.proxies, Component: ProxyWorkspace },
  { ...navigationItems.subscriptions, Component: SubscriptionWorkspace },
  { ...navigationItems.plugins, Component: PluginsPage },
  { ...navigationItems.settings, Component: SettingsWorkspace },
]

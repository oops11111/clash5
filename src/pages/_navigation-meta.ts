export const navigationItems = {
  home: { label: 'layout.components.navigation.tabs.home', path: '/' },
  proxies: {
    label: 'layout.components.navigation.tabs.proxies',
    path: '/proxies',
  },
  subscriptions: {
    label: 'layout.components.navigation.tabs.profiles',
    path: '/subscriptions',
  },
  plugins: {
    label: 'layout.components.navigation.tabs.plugins',
    path: '/plugins',
  },
  connections: {
    label: 'layout.components.navigation.tabs.connections',
    path: '/connections',
  },
  logs: { label: 'layout.components.navigation.tabs.logs', path: '/logs' },
  settings: {
    label: 'layout.components.navigation.tabs.settings',
    path: '/settings',
  },
} as const

export const navigationItems = {
  home: { label: 'layout.components.navigation.tabs.home', path: '/' },
  nodePool: {
    label: 'layout.components.navigation.tabs.nodePool',
    path: '/nodes',
  },
  profiles: {
    label: 'layout.components.navigation.tabs.profiles',
    path: '/profile',
  },
  chains: {
    label: 'layout.components.navigation.tabs.chains',
    path: '/chains',
  },
  rules: { label: 'layout.components.navigation.tabs.rules', path: '/rules' },
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

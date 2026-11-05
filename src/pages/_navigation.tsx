import DnsOutlinedIcon from '@mui/icons-material/DnsOutlined'
import ExtensionOutlinedIcon from '@mui/icons-material/ExtensionOutlined'
import HomeOutlinedIcon from '@mui/icons-material/HomeOutlined'
import SettingsOutlinedIcon from '@mui/icons-material/SettingsOutlined'
import SwapHorizOutlinedIcon from '@mui/icons-material/SwapHorizOutlined'
import { type ComponentType, type ReactNode } from 'react'

import HomeSvg from '@/assets/image/itemicon/home.svg?react'
import ProfilesSvg from '@/assets/image/itemicon/profiles.svg?react'
import ProxiesSvg from '@/assets/image/itemicon/proxies.svg?react'
import SettingsSvg from '@/assets/image/itemicon/settings.svg?react'

import { navigationItems, primaryNavigationItems } from './_navigation-meta'
import HomePage from './home'
import PluginsPage from './plugins'
import ProfilePage from './profiles'
import ProxyPage from './proxies'
import SettingPage from './settings'

type NavigationItem = {
  label: (typeof navigationItems)[keyof typeof navigationItems]['label']
  path: string
  icon: ReactNode[]
  Component: ComponentType
}

export const navItems: NavigationItem[] = [
  {
    ...primaryNavigationItems.home,
    icon: [<HomeOutlinedIcon key="mui" />, <HomeSvg key="svg" />],
    Component: HomePage,
  },
  {
    ...primaryNavigationItems.proxies,
    icon: [<SwapHorizOutlinedIcon key="mui" />, <ProxiesSvg key="svg" />],
    Component: ProxyPage,
  },
  {
    ...primaryNavigationItems.profiles,
    icon: [<DnsOutlinedIcon key="mui" />, <ProfilesSvg key="svg" />],
    Component: ProfilePage,
  },
  {
    ...primaryNavigationItems.plugins,
    icon: [
      <ExtensionOutlinedIcon key="mui" />,
      <ExtensionOutlinedIcon key="color" color="primary" />,
    ],
    Component: PluginsPage,
  },
  {
    ...primaryNavigationItems.settings,
    icon: [<SettingsOutlinedIcon key="mui" />, <SettingsSvg key="svg" />],
    Component: SettingPage,
  },
]

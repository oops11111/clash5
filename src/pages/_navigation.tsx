import DnsOutlinedIcon from '@mui/icons-material/DnsOutlined'
import ExtensionOutlinedIcon from '@mui/icons-material/ExtensionOutlined'
import ForkRightOutlinedIcon from '@mui/icons-material/ForkRightOutlined'
import HomeOutlinedIcon from '@mui/icons-material/HomeOutlined'
import HubOutlinedIcon from '@mui/icons-material/HubOutlined'
import LanguageOutlinedIcon from '@mui/icons-material/LanguageOutlined'
import SettingsOutlinedIcon from '@mui/icons-material/SettingsOutlined'
import { type ComponentType, type ReactNode } from 'react'

import ConnectionsSvg from '@/assets/image/itemicon/connections.svg?react'
import HomeSvg from '@/assets/image/itemicon/home.svg?react'
import ProfilesSvg from '@/assets/image/itemicon/profiles.svg?react'
import ProxiesSvg from '@/assets/image/itemicon/proxies.svg?react'
import RulesSvg from '@/assets/image/itemicon/rules.svg?react'
import SettingsSvg from '@/assets/image/itemicon/settings.svg?react'

import { navigationItems } from './_navigation-meta'
import ChainsPage from './chains'
import DiagnosticsPage from './diagnostics'
import HomePage from './home'
import NodePoolPage from './node-pool'
import PluginsPage from './plugins'
import ProfilePage from './profiles'
import RulesPage from './rules'
import SettingPage from './settings'

type NavigationItem = {
  label: (typeof navigationItems)[keyof typeof navigationItems]['label']
  path: string
  icon: ReactNode[]
  Component: ComponentType
}

export const navItems: NavigationItem[] = [
  {
    ...navigationItems.home,
    icon: [<HomeOutlinedIcon key="mui" />, <HomeSvg key="svg" />],
    Component: HomePage,
  },
  {
    ...navigationItems.nodePool,
    icon: [<DnsOutlinedIcon key="mui" />, <ProxiesSvg key="svg" />],
    Component: NodePoolPage,
  },
  {
    ...navigationItems.profiles,
    icon: [<DnsOutlinedIcon key="mui" />, <ProfilesSvg key="svg" />],
    Component: ProfilePage,
  },
  {
    ...navigationItems.chains,
    icon: [
      <HubOutlinedIcon key="mui" />,
      <HubOutlinedIcon key="color" color="primary" />,
    ],
    Component: ChainsPage,
  },
  {
    ...navigationItems.rules,
    icon: [<ForkRightOutlinedIcon key="mui" />, <RulesSvg key="svg" />],
    Component: RulesPage,
  },
  {
    ...navigationItems.plugins,
    icon: [
      <ExtensionOutlinedIcon key="mui" />,
      <ExtensionOutlinedIcon key="color" color="primary" />,
    ],
    Component: PluginsPage,
  },
  {
    ...navigationItems.diagnostics,
    icon: [<LanguageOutlinedIcon key="mui" />, <ConnectionsSvg key="svg" />],
    Component: DiagnosticsPage,
  },
  {
    ...navigationItems.settings,
    icon: [<SettingsOutlinedIcon key="mui" />, <SettingsSvg key="svg" />],
    Component: SettingPage,
  },
]

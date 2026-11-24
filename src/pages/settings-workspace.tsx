import ConnectionsPage from './connections'
import LogsPage from './logs'
import SettingPage from './settings'
import UnlockPage from './unlock'
import { WorkspaceTabs } from './workspace-tabs'

const SettingsWorkspace = () => (
  <WorkspaceTabs
    defaultSection="general"
    sections={[
      {
        id: 'general',
        label: 'layout.components.navigation.tabs.settings',
        Component: SettingPage,
      },
      {
        id: 'connections',
        label: 'layout.components.navigation.tabs.connections',
        Component: ConnectionsPage,
      },
      {
        id: 'logs',
        label: 'layout.components.navigation.tabs.logs',
        Component: LogsPage,
      },
      {
        id: 'unlock',
        label: 'layout.components.navigation.tabs.unlock',
        Component: UnlockPage,
      },
    ]}
  />
)

export default SettingsWorkspace

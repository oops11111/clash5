import ProxyPage from './proxies'
import RulesPage from './rules'
import { WorkspaceTabs } from './workspace-tabs'

const ProxyWorkspace = () => (
  <WorkspaceTabs
    defaultSection="proxies"
    sections={[
      {
        id: 'proxies',
        label: 'layout.components.navigation.tabs.proxies',
        Component: ProxyPage,
      },
      {
        id: 'rules',
        label: 'layout.components.navigation.tabs.rules',
        Component: RulesPage,
      },
    ]}
  />
)

export default ProxyWorkspace

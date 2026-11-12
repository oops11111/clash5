import NodePoolPage from './node-pool'
import ProfilePage from './profiles'
import { WorkspaceTabs } from './workspace-tabs'

const SubscriptionWorkspace = () => (
  <WorkspaceTabs
    defaultSection="sources"
    sections={[
      {
        id: 'sources',
        label: 'layout.components.navigation.tabs.profiles',
        Component: ProfilePage,
      },
      {
        id: 'nodes',
        label: 'layout.components.navigation.tabs.nodePool',
        Component: NodePoolPage,
      },
    ]}
  />
)

export default SubscriptionWorkspace

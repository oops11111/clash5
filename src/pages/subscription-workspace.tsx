import NodePoolPage from './node-pool'
import ProfilePage from './profiles'
import { WorkspaceTabs } from './workspace-tabs'

const SourcesPane = () => <ProfilePage compact />
const NodesPane = () => <NodePoolPage compact />

// `section` means the currently displayed content, like every other
// workspace: the two panes are tabs, not a highlighted dual pane.
const SubscriptionWorkspace = () => (
  <WorkspaceTabs
    defaultSection="sources"
    sections={[
      {
        id: 'sources',
        label: 'layout.components.navigation.tabs.profiles',
        Component: SourcesPane,
        testId: 'subscriptions-source-pane',
      },
      {
        id: 'nodes',
        label: 'layout.components.navigation.tabs.nodePool',
        Component: NodesPane,
        testId: 'subscriptions-node-pane',
      },
    ]}
  />
)

export default SubscriptionWorkspace

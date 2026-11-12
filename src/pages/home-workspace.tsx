import ChainsPage from './chains'
import HomePage from './home'
import { WorkspaceTabs } from './workspace-tabs'

const HomeWorkspace = () => (
  <WorkspaceTabs
    defaultSection="overview"
    sections={[
      { id: 'overview', label: 'home.page.title', Component: HomePage },
      {
        id: 'chains',
        label: 'layout.components.navigation.tabs.chains',
        Component: ChainsPage,
      },
    ]}
  />
)

export default HomeWorkspace

import ChainsPage from './chains'
import HomePage from './home'
import HomeDetailsPage from './home-details'
import { WorkspaceTabs } from './workspace-tabs'

const HomeWorkspace = () => (
  <WorkspaceTabs
    defaultSection="overview"
    sections={[
      { id: 'overview', label: 'home.page.title', Component: HomePage },
      {
        id: 'details',
        label: 'home.page.tabs.details',
        Component: HomeDetailsPage,
      },
      {
        id: 'chains',
        label: 'layout.components.navigation.tabs.chains',
        Component: ChainsPage,
      },
    ]}
  />
)

export default HomeWorkspace

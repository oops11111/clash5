import { Box } from '@mui/material'
import { useSearchParams } from 'react-router'

import NodePoolPage from './node-pool'
import ProfilePage from './profiles'

const SubscriptionWorkspace = () => {
  const [searchParams] = useSearchParams()
  const focusedPane = searchParams.get('section') ?? 'sources'

  return (
    <Box className="subscription-workspace" data-focused-pane={focusedPane}>
      <Box
        className="subscription-workspace__source"
        data-pane="sources"
        data-testid="subscriptions-source-pane"
      >
        <ProfilePage compact />
      </Box>
      <Box
        className="subscription-workspace__nodes"
        data-pane="nodes"
        data-testid="subscriptions-node-pane"
      >
        <NodePoolPage compact />
      </Box>
    </Box>
  )
}

export default SubscriptionWorkspace

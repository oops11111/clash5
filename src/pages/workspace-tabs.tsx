import { Box, Tab, Tabs } from '@mui/material'
import { useTranslation } from 'react-i18next'
import { useNavigate, useSearchParams } from 'react-router'

type WorkspaceSection = {
  id: string
  label: string
  Component: React.ComponentType
}

export const WorkspaceTabs = ({
  sections,
  defaultSection,
}: {
  sections: WorkspaceSection[]
  defaultSection: string
}) => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const [searchParams] = useSearchParams()
  const requested = searchParams.get('section') ?? defaultSection
  const section = sections.some(({ id }) => id === requested)
    ? requested
    : defaultSection
  const ActivePage =
    sections.find(({ id }) => id === section)?.Component ??
    sections[0].Component

  return (
    <Box className="workspace-tabs">
      <Tabs
        value={section}
        onChange={(_, value: string) => {
          const next = new URLSearchParams(searchParams)
          if (value === defaultSection) next.delete('section')
          else next.set('section', value)
          navigate({ search: next.toString() }, { replace: false })
        }}
        variant="scrollable"
        scrollButtons="auto"
      >
        {sections.map(({ id, label }) => (
          <Tab key={id} value={id} label={t(label)} />
        ))}
      </Tabs>
      <Box className="workspace-tabs__page">
        <ActivePage />
      </Box>
    </Box>
  )
}

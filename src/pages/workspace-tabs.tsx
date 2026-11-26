import { Box, Tab, Tabs } from '@mui/material'
import { useId } from 'react'
import { useTranslation } from 'react-i18next'
import { useNavigate, useSearchParams } from 'react-router'

type WorkspaceSection = {
  id: string
  label: string
  Component: React.ComponentType
  testId?: string
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
  const tabsId = useId()
  const [searchParams] = useSearchParams()
  const requested = searchParams.get('section') ?? defaultSection
  const section = sections.some(({ id }) => id === requested)
    ? requested
    : defaultSection
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
          <Tab
            key={id}
            id={`${tabsId}-tab-${id}`}
            aria-controls={`${tabsId}-panel-${id}`}
            value={id}
            label={t(label)}
          />
        ))}
      </Tabs>
      {sections.map(({ id, Component, testId }) => (
        <Box
          key={id}
          className="workspace-tabs__page"
          role="tabpanel"
          id={`${tabsId}-panel-${id}`}
          aria-labelledby={`${tabsId}-tab-${id}`}
          data-testid={testId}
          hidden={section !== id}
        >
          {section === id && <Component />}
        </Box>
      ))}
    </Box>
  )
}

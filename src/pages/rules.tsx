import { Box, Button } from '@mui/material'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import {
  BaseEmpty,
  BasePage,
  BaseSearchBox,
  VirtualList,
  type VirtualListHandle,
} from '@/components/base'
import { ScrollTopButton } from '@/components/layout/scroll-top-button'
import { RulesEditorViewer } from '@/components/profile/rules-editor-viewer'
import { ProviderButton } from '@/components/rule/provider-button'
import RuleItem from '@/components/rule/rule-item'
import { useProfiles } from '@/hooks/use-profiles'
import { useVisibility } from '@/hooks/use-visibility'
import { useAppRefreshers, useRulesData } from '@/providers/app-data-context'
import { enhanceProfiles } from '@/services/cmds'
import { mutate } from '@/services/mutate'
import { showNotice } from '@/services/notice-service'

const RulesPage = () => {
  const { t } = useTranslation()
  const { rules = [] } = useRulesData()
  const { refreshRules, refreshRuleProviders } = useAppRefreshers()
  const { current } = useProfiles()
  const [match, setMatch] = useState(() => (_: string) => true)
  const virtuosoRef = useRef<VirtualListHandle>(null)
  const [showScrollTop, setShowScrollTop] = useState(false)
  const pageVisible = useVisibility()
  const [editorOpen, setEditorOpen] = useState(false)
  const [editorPasteOpen, setEditorPasteOpen] = useState(false)

  // 在组件挂载时和页面获得焦点时刷新规则数据
  useEffect(() => {
    refreshRules()
    refreshRuleProviders()

    if (pageVisible) {
      refreshRules()
      refreshRuleProviders()
    }
  }, [refreshRules, refreshRuleProviders, pageVisible])

  const filteredRules = useMemo(() => {
    const rulesWithLineNo = rules.map((item, index) => ({
      ...item,
      // UI-only derived data; keep app context/SWR data immutable
      lineNo: index + 1,
    }))

    return rulesWithLineNo.filter((item) => match(item.payload ?? ''))
  }, [rules, match])

  const handleScroll = useCallback((e: Event) => {
    setShowScrollTop((e.target as HTMLElement).scrollTop > 100)
  }, [])

  const scrollToTop = () => {
    virtuosoRef.current?.scrollTo({ top: 0, behavior: 'smooth' })
  }

  const rulesProperty = current?.option?.rules
  const openEditor = (pasteOpen: boolean) => {
    setEditorPasteOpen(pasteOpen)
    setEditorOpen(true)
  }

  // Same save/enhance chain as the subscription-card menu entry: the dialog
  // saves the chain file, then the current profile is re-enhanced.
  const handleEditorSave = useCallback(
    async (prev?: string, curr?: string) => {
      if (prev === curr) return
      try {
        const result = await mutate(() => enhanceProfiles(), {
          id: 'enhance-profiles',
          errorNotice: false,
        })
        if (result.ok && result.value === true) {
          await Promise.all([refreshRules(), refreshRuleProviders()])
        }
      } catch (error) {
        showNotice.error(error)
      }
    },
    [refreshRules, refreshRuleProviders],
  )

  return (
    <BasePage
      full
      title={t('rules.page.title')}
      contentStyle={{
        height: '100%',
        display: 'flex',
        flexDirection: 'column',
        overflow: 'auto',
      }}
      header={
        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
          <ProviderButton />
          <Button
            size="small"
            variant="outlined"
            disabled={!rulesProperty}
            onClick={() => openEditor(false)}
          >
            {t('profiles.components.menu.editRules')}
          </Button>
          <Button
            size="small"
            variant="outlined"
            disabled={!rulesProperty}
            onClick={() => openEditor(true)}
          >
            {t('rules.modals.editor.paste.title')}
          </Button>
        </Box>
      }
    >
      <Box
        sx={{
          pt: 1,
          mb: 0.5,
          mx: '10px',
          height: '36px',
          display: 'flex',
          alignItems: 'center',
        }}
      >
        <BaseSearchBox onSearch={(match) => setMatch(() => match)} />
      </Box>

      {filteredRules && filteredRules.length > 0 ? (
        <>
          <VirtualList
            ref={virtuosoRef}
            count={filteredRules.length}
            estimateSize={40}
            renderItem={(i) => <RuleItem value={filteredRules[i]} />}
            style={{ flex: 1 }}
            onScroll={handleScroll}
          />
          <ScrollTopButton onClick={scrollToTop} show={showScrollTop} />
        </>
      ) : (
        <BaseEmpty />
      )}

      {editorOpen && current && rulesProperty && (
        <RulesEditorViewer
          groupsUid={current.option?.groups ?? ''}
          mergeUid={current.option?.merge ?? ''}
          profileUid={current.uid}
          property={rulesProperty}
          open={true}
          initialPasteOpen={editorPasteOpen}
          onSave={handleEditorSave}
          onClose={() => setEditorOpen(false)}
        />
      )}
    </BasePage>
  )
}

export default RulesPage

import { ExpandMoreRounded } from '@mui/icons-material'
import {
  Accordion,
  AccordionDetails,
  AccordionSummary,
  Box,
  Typography,
} from '@mui/material'
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
import { ProviderButton } from '@/components/rule/provider-button'
import RuleItem from '@/components/rule/rule-item'
import { TextRuleSets } from '@/components/rule/text-rule-sets'
import { useVisibility } from '@/hooks/use-visibility'
import { useAppRefreshers, useRulesData } from '@/providers/app-data-context'
import { getClash5RuleOverview } from '@/services/cmds'
import type {
  RuleNote,
  RuleOverview,
} from '@/services/tauri-commands.generated'
import { ruleKind } from '@/utils/text-rules'

const RulesPage = () => {
  const { t } = useTranslation()
  const { rules: rawRules } = useRulesData()
  const rules = useMemo(() => rawRules ?? [], [rawRules])
  const { refreshRules, refreshRuleProviders } = useAppRefreshers()
  const [match, setMatch] = useState(() => (_: string) => true)
  const virtuosoRef = useRef<VirtualListHandle>(null)
  const [showScrollTop, setShowScrollTop] = useState(false)
  const pageVisible = useVisibility()
  const [overview, setOverview] = useState<RuleOverview | null>(null)
  const notes = useMemo<RuleNote[]>(() => overview?.notes ?? [], [overview])

  // The overview is produced when the config is built, so it refreshes with the rule list.
  useEffect(() => {
    getClash5RuleOverview()
      .then(setOverview)
      .catch(() => setOverview(null))
  }, [rules])

  const noteByRule = useMemo(
    () =>
      new Map(
        notes.map((note) => [
          `${note.kind},${note.payload},${note.policy}`,
          note,
        ]),
      ),
    [notes],
  )

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
        </Box>
      }
    >
      <Accordion disableGutters variant="outlined" sx={{ mx: '10px', mt: 1 }}>
        <AccordionSummary expandIcon={<ExpandMoreRounded />}>
          <Typography sx={{ fontWeight: 600 }}>
            {t('rules.textRules.title')}
          </Typography>
        </AccordionSummary>
        <AccordionDetails>
          <TextRuleSets rules={rules} />
        </AccordionDetails>
      </Accordion>

      {overview && overview.subscription_rules > 0 && (
        <Typography
          variant="body2"
          color="text.secondary"
          sx={{ mx: '10px', mt: 1 }}
        >
          {overview.decoupled
            ? t('rules.overview.decoupled', {
                count: overview.subscription_rules,
              })
            : t('rules.overview.kept', { count: overview.subscription_rules })}
        </Typography>
      )}

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
            renderItem={(i) => {
              const rule = filteredRules[i]
              const note = noteByRule.get(
                `${ruleKind(rule.type)},${rule.payload ?? ''},${rule.proxy}`,
              )
              return <RuleItem value={rule} note={note} />
            }}
            style={{ flex: 1 }}
            onScroll={handleScroll}
          />
          <ScrollTopButton onClick={scrollToTop} show={showScrollTop} />
        </>
      ) : (
        <BaseEmpty />
      )}
    </BasePage>
  )
}

export default RulesPage

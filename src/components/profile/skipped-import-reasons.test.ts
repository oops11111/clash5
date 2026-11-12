import { createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { SkippedImportReasons } from './skipped-import-reasons'

it('renders every backend skip reason verbatim as a separate UI item', () => {
  const reasons = [
    'Unsupported merge profile',
    'External provider profile',
    'profile cache is invalid YAML',
  ]
  const html = renderToStaticMarkup(
    createElement(SkippedImportReasons, { reasons }),
  )

  for (const reason of reasons) expect(html).toContain(reason)
  expect(html.match(/data-testid="skipped-import-reason"/g)).toHaveLength(
    reasons.length,
  )
})

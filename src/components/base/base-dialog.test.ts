import { Button, DialogActions } from '@mui/material'
import { isValidElement, type ReactElement, type ReactNode } from 'react'
import { describe, expect, it } from 'vitest'

import { BaseDialog } from './base-dialog'

const getConfirmButton = (okColor?: 'error' | 'primary') => {
  const dialog = BaseDialog({
    open: true,
    title: 'Confirm action',
    okBtn: 'Confirm',
    cancelBtn: 'Cancel',
    okColor,
  })

  if (!isValidElement<{ children?: ReactNode }>(dialog)) {
    throw new Error('Dialog was not rendered')
  }

  const children = dialog.props.children as ReactNode[]
  const actions = children[2]
  if (
    !isValidElement<{ children?: ReactNode }>(actions) ||
    actions.type !== DialogActions
  ) {
    throw new Error('Dialog actions were not rendered')
  }

  const buttons = actions.props.children as ReactNode[]
  const button = buttons.at(-1)
  if (!isValidElement<{ color?: 'error' | 'primary' }>(button)) {
    throw new Error('Confirmation button was not rendered')
  }

  expect(button.type).toBe(Button)
  return button as ReactElement<{ color?: 'error' | 'primary' }>
}

describe('BaseDialog confirmation button', () => {
  it('uses error color when a destructive confirmation requests it', () => {
    expect(getConfirmButton('error').props.color).toBe('error')
  })

  it('keeps primary color by default', () => {
    expect(getConfirmButton().props.color).toBe('primary')
  })
})

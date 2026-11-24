import getSystem from '@/utils/get-system'
const OS = getSystem()

// default theme setting
export const defaultTheme = {
  primary_color: '#2F6BFF',
  secondary_color: '#8B5CF6',
  primary_text: '#1A2332',
  secondary_text: '#5A6B82',
  info_color: '#2F6BFF',
  error_color: '#EF4444',
  warning_color: '#F59E0B',
  success_color: '#22C55E',
  background_color: '#FFFFFF',
  font_family: `"Segoe UI", "PingFang SC", "Microsoft YaHei", -apple-system, BlinkMacSystemFont, Roboto, "Helvetica Neue", Arial, sans-serif, "Apple Color Emoji"${
    OS === 'windows' ? ', twemoji mozilla' : ''
  }`,
}

// dark mode
export const defaultDarkTheme = {
  ...defaultTheme,
  primary_color: '#0A84FF',
  secondary_color: '#FF9F0A',
  primary_text: '#FFFFFF',
  background_color: '#2E303D',
  secondary_text: '#EBEBF599',
  info_color: '#0A84FF',
  error_color: '#FF453A',
  warning_color: '#FF9F0A',
  success_color: '#30D158',
}

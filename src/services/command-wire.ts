import type {
  IProfiles_Serialize,
  IVerge_Serialize,
  NodeRecord,
  ProxyViewV1_Serialize,
} from '@/services/tauri-commands.generated'
import type { Clash5NodeRecord } from '@/types/clash5-node'
import type { ProxyViewV1 } from '@/types/proxy-view'

type WithoutNulls<T> = T extends null
  ? undefined
  : T extends (infer Item)[]
    ? WithoutNulls<Item>[]
    : T extends object
      ? { [Key in keyof T]: WithoutNulls<T[Key]> }
      : T

// Legacy UI configuration models use undefined for absent values. Only use this
// at those boundaries; null remains meaningful in node quality and proxy views.
export function withoutNulls<T>(value: T): WithoutNulls<T> {
  const normalized =
    value === null
      ? undefined
      : Array.isArray(value)
        ? value.map(withoutNulls)
        : typeof value === 'object' && value !== null
          ? Object.fromEntries(
              Object.entries(value).map(([key, item]) => [
                key,
                withoutNulls(item),
              ]),
            )
          : value
  return normalized as WithoutNulls<T>
}

function choice<T extends string | number>(
  value: unknown,
  allowed: readonly T[],
): T | undefined {
  return allowed.find((item) => item === value)
}

export function profilesFromWire(value: IProfiles_Serialize): IProfilesConfig {
  const profiles = withoutNulls(value)
  return {
    current: profiles.current,
    items: profiles.items?.map((item) => {
      if (!item.uid) throw new Error('Profile response is missing its uid')
      return {
        ...item,
        uid: item.uid,
        type: choice(item.type, [
          'local',
          'remote',
          'merge',
          'script',
        ] as const),
      }
    }),
  }
}

export function vergeFromWire(value: IVerge_Serialize): IVergeConfig {
  const config = withoutNulls(value)
  const homeCards: Record<string, boolean> = {}
  if (
    config.home_cards &&
    typeof config.home_cards === 'object' &&
    !Array.isArray(config.home_cards)
  ) {
    for (const [key, enabled] of Object.entries(config.home_cards)) {
      if (typeof enabled === 'boolean') homeCards[key] = enabled
    }
  }
  return {
    ...config,
    theme_mode: choice(config.theme_mode, ['light', 'dark', 'system'] as const),
    menu_icon: choice(config.menu_icon, [
      'monochrome',
      'colorful',
      'disable',
    ] as const),
    tray_icon: choice(config.tray_icon, ['monochrome', 'colorful'] as const),
    proxy_group_tools_position: choice(config.proxy_group_tools_position, [
      'left',
      'right',
    ] as const),
    notice_position: choice(config.notice_position, [
      'top-left',
      'top-right',
      'bottom-left',
      'bottom-right',
    ] as const),
    tray_proxy_groups_display_mode: choice(
      config.tray_proxy_groups_display_mode,
      ['default', 'inline', 'disable'] as const,
    ),
    auto_log_clean: choice(config.auto_log_clean, [0, 1, 2, 3, 4] as const),
    home_cards: config.home_cards == null ? undefined : homeCards,
    test_list: config.test_list?.flatMap((item) =>
      item.uid && item.url ? [{ ...item, uid: item.uid, url: item.url }] : [],
    ),
  }
}

export function nodeFromWire(node: NodeRecord): Clash5NodeRecord {
  const quality =
    node.quality === null
      ? null
      : {
          ...node.quality,
          failure_streak: node.quality.failure_streak ?? 0,
        }
  return { ...node, quality }
}

export function proxyViewFromWire(view: ProxyViewV1_Serialize): ProxyViewV1 {
  if (view.schemaVersion !== 1) {
    throw new Error('Unsupported proxy view schema: ' + view.schemaVersion)
  }
  return {
    ...withoutNulls(view),
    schemaVersion: 1,
    global: view.global === null ? null : withoutNulls(view.global),
    direct: view.direct,
  }
}

type NodeAvailability = { active: boolean }

export const shouldShowFirstRunEmptyState = (
  nodes: readonly NodeAvailability[] | null,
  hasError: boolean,
) => nodes !== null && !hasError && !nodes.some((node) => node.active)

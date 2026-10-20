import { useSystemState } from '@/hooks/use-system-state'
import { getMemoryMitigationStatus } from '@/services/cmds'
import { useQuery } from '@/services/query-client'

export function useMemoryMitigation() {
  const { runningMode } = useSystemState()
  const { data } = useQuery({
    queryKey: ['memoryMitigationStatus', runningMode],
    queryFn: getMemoryMitigationStatus,
    refetchInterval: 15000,
    refetchOnWindowFocus: true,
    retry: 1,
  })

  return data
}

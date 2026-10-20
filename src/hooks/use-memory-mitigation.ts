import { getMemoryMitigationStatus } from '@/services/cmds'
import { useQuery } from '@/services/query-client'

export function useMemoryMitigation() {
  const { data } = useQuery({
    queryKey: ['memoryMitigationStatus'],
    queryFn: getMemoryMitigationStatus,
    refetchInterval: 15000,
    refetchOnWindowFocus: true,
    retry: 1,
  })

  return data
}

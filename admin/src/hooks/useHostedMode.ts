import { useQuery } from '@tanstack/react-query';
import { api } from '@api/client';

export function useHostedMode(): boolean {
  const { data } = useQuery({
    queryKey: ['hosted-mode'],
    queryFn: api.getHostedMode,
    staleTime: Infinity,
  });
  return data?.hosted_mode ?? false;
}

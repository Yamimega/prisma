"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";

export function useBandwidthSummary() {
  return useQuery({
    queryKey: ["bandwidth-summary"],
    queryFn: api.getBandwidthSummary,
    refetchInterval: 5000,
  });
}

export function useClientBandwidth(id: string) {
  return useQuery({
    queryKey: ["client-bandwidth", id],
    queryFn: () => api.getClientBandwidth(id),
    refetchInterval: 5000,
    enabled: !!id,
  });
}

export function useUpdateClientBandwidth() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: { upload_bps?: number; download_bps?: number } }) =>
      api.updateClientBandwidth(id, data),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ["client-bandwidth", vars.id] });
      qc.invalidateQueries({ queryKey: ["bandwidth-summary"] });
    },
  });
}

export function useClientQuota(id: string) {
  return useQuery({
    queryKey: ["client-quota", id],
    queryFn: () => api.getClientQuota(id),
    refetchInterval: 5000,
    enabled: !!id,
  });
}

export function useUpdateClientQuota() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, data }: { id: string; data: { quota_bytes?: number } }) =>
      api.updateClientQuota(id, data),
    onSuccess: (_data, vars) => {
      qc.invalidateQueries({ queryKey: ["client-quota", vars.id] });
      qc.invalidateQueries({ queryKey: ["bandwidth-summary"] });
    },
  });
}

"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/api";

export function useSystemInfo() {
  return useQuery({
    queryKey: ["system-info"],
    queryFn: api.getSystemInfo,
    refetchInterval: 30000,
  });
}

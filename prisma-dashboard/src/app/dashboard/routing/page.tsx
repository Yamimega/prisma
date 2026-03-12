"use client";

import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { RuleList } from "@/components/routing/rule-list";
import { RuleEditor } from "@/components/routing/rule-editor";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { RoutingRule } from "@/lib/types";

export default function RoutingPage() {
  const queryClient = useQueryClient();

  const { data: routes, isLoading } = useQuery({
    queryKey: ["routes"],
    queryFn: api.getRoutes,
  });

  const createRoute = useMutation({
    mutationFn: (data: Omit<RoutingRule, "id">) => api.createRoute(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["routes"] });
    },
  });

  const updateRoute = useMutation({
    mutationFn: ({ id, data }: { id: string; data: Omit<RoutingRule, "id"> }) =>
      api.updateRoute(id, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["routes"] });
    },
  });

  const deleteRoute = useMutation({
    mutationFn: (id: string) => api.deleteRoute(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["routes"] });
    },
  });

  function handleToggle(id: string, enabled: boolean) {
    const rule = routes?.find((r) => r.id === id);
    if (!rule) return;
    const { id: _id, ...rest } = rule;
    updateRoute.mutate({ id, data: { ...rest, enabled } });
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Routing Rules</h2>
        <RuleEditor
          onSubmit={(rule) => createRoute.mutate(rule)}
          isLoading={createRoute.isPending}
        />
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Rules</CardTitle>
        </CardHeader>
        <CardContent>
          {isLoading ? (
            <div className="flex items-center justify-center py-12">
              <p className="text-sm text-muted-foreground">Loading routes...</p>
            </div>
          ) : (
            <RuleList
              rules={routes ?? []}
              onToggle={handleToggle}
              onDelete={(id) => deleteRoute.mutate(id)}
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}

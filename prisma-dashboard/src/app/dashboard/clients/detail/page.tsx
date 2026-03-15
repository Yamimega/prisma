"use client";

import { useSearchParams } from "next/navigation";
import { Suspense } from "react";
import ClientDetailPage from "@/components/clients/client-detail";

function ClientDetailContent() {
  const searchParams = useSearchParams();
  const id = searchParams.get("id");

  if (!id) {
    return (
      <div className="flex items-center justify-center py-12">
        <p className="text-sm text-muted-foreground">No client ID specified</p>
      </div>
    );
  }

  return <ClientDetailPage clientId={id} />;
}

export default function Page() {
  return (
    <Suspense fallback={<div className="flex items-center justify-center py-12"><p className="text-sm text-muted-foreground">Loading...</p></div>}>
      <ClientDetailContent />
    </Suspense>
  );
}

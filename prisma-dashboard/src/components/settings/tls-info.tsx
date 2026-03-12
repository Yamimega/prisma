import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { TlsInfoResponse } from "@/lib/types";

interface TlsInfoProps {
  tls: TlsInfoResponse;
}

export function TlsInfo({ tls }: TlsInfoProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>TLS Configuration</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex items-center gap-2">
          <span className="text-sm text-muted-foreground">Status:</span>
          {tls.enabled ? (
            <Badge className="bg-green-500/15 text-green-700 dark:text-green-400">
              Enabled
            </Badge>
          ) : (
            <Badge className="bg-red-500/15 text-red-700 dark:text-red-400">
              Disabled
            </Badge>
          )}
        </div>
        <div>
          <p className="text-sm text-muted-foreground">Certificate Path</p>
          <p className="text-sm font-mono">
            {tls.cert_path ?? "Not configured"}
          </p>
        </div>
        <div>
          <p className="text-sm text-muted-foreground">Key Path</p>
          <p className="text-sm font-mono">
            {tls.key_path ?? "Not configured"}
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

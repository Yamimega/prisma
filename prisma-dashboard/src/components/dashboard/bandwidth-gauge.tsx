"use client";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { formatBytes } from "@/lib/utils";

interface BandwidthGaugeProps {
  currentBps: number;
  maxBps: number;
  label?: string;
}

export function BandwidthGauge({ currentBps, maxBps, label = "Bandwidth" }: BandwidthGaugeProps) {
  const percentage = maxBps > 0 ? Math.min((currentBps / maxBps) * 100, 100) : 0;
  const radius = 60;
  const strokeWidth = 10;
  const circumference = 2 * Math.PI * radius;
  const dashOffset = circumference - (percentage / 100) * circumference;

  const color =
    percentage >= 90
      ? "hsl(0, 72%, 51%)"
      : percentage >= 70
        ? "hsl(38, 92%, 50%)"
        : "hsl(142, 71%, 45%)";

  return (
    <Card>
      <CardHeader>
        <CardTitle>{label}</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col items-center">
        <svg width={160} height={160} viewBox="0 0 160 160">
          {/* Background circle */}
          <circle
            cx="80"
            cy="80"
            r={radius}
            fill="none"
            stroke="hsl(var(--muted))"
            strokeWidth={strokeWidth}
          />
          {/* Progress circle */}
          <circle
            cx="80"
            cy="80"
            r={radius}
            fill="none"
            stroke={color}
            strokeWidth={strokeWidth}
            strokeDasharray={circumference}
            strokeDashoffset={dashOffset}
            strokeLinecap="round"
            transform="rotate(-90 80 80)"
            className="transition-all duration-500"
          />
          {/* Percentage text */}
          <text
            x="80"
            y="75"
            textAnchor="middle"
            className="fill-foreground text-xl font-semibold"
            fontSize="20"
          >
            {percentage.toFixed(0)}%
          </text>
          {/* Rate text */}
          <text
            x="80"
            y="95"
            textAnchor="middle"
            className="fill-muted-foreground"
            fontSize="11"
          >
            {formatBytes(currentBps)}/s
          </text>
        </svg>
        {maxBps > 0 && (
          <p className="mt-2 text-xs text-muted-foreground">
            Max: {formatBytes(maxBps)}/s
          </p>
        )}
      </CardContent>
    </Card>
  );
}

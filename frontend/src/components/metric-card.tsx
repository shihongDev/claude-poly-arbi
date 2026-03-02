import { Card, CardContent } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { ArrowUp, ArrowDown, Minus } from "lucide-react";

interface MetricCardProps {
  title: string;
  value: string;
  delta?: string;
  deltaType?: "positive" | "negative" | "neutral";
}

const deltaIcons = {
  positive: ArrowUp,
  negative: ArrowDown,
  neutral: Minus,
};

const deltaColors = {
  positive: "text-emerald-500",
  negative: "text-red-500",
  neutral: "text-zinc-400",
};

export function MetricCard({ title, value, delta, deltaType }: MetricCardProps) {
  const DeltaIcon = deltaType ? deltaIcons[deltaType] : null;

  return (
    <Card className="border-zinc-800 bg-zinc-900 py-5">
      <CardContent className="px-5 py-0">
        <p className="text-xs font-medium uppercase tracking-wider text-zinc-400">
          {title}
        </p>
        <p className="mt-2 text-2xl font-bold text-white" style={{ fontFamily: "var(--font-mono)" }}>
          {value}
        </p>
        {delta && deltaType && DeltaIcon && (
          <div className={cn("mt-1 flex items-center gap-1 text-sm", deltaColors[deltaType])}>
            <DeltaIcon className="h-3 w-3" />
            <span style={{ fontFamily: "var(--font-mono)" }}>{delta}</span>
          </div>
        )}
      </CardContent>
    </Card>
  );
}

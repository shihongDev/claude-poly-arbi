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
  positive: "text-[#2D6A4F]",
  negative: "text-[#B44C3F]",
  neutral: "text-[#9B9B9B]",
};

export function MetricCard({ title, value, delta, deltaType }: MetricCardProps) {
  const DeltaIcon = deltaType ? deltaIcons[deltaType] : null;

  return (
    <div className="py-5 px-5">
      <p className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        {title}
      </p>
      <p className="mt-2 text-4xl font-bold text-[#1A1A19]" style={{ fontFamily: "var(--font-space-grotesk)" }}>
        {value}
      </p>
      {delta && deltaType && DeltaIcon && (
        <div className={cn("mt-1 flex items-center gap-1 text-sm", deltaColors[deltaType])}>
          <DeltaIcon className="h-3 w-3" />
          <span style={{ fontFamily: "var(--font-jetbrains-mono)" }}>{delta}</span>
        </div>
      )}
    </div>
  );
}

"use client";

import { cn } from "@/lib/utils";
import { LayoutList, LayoutGrid, BarChart3 } from "lucide-react";

type ViewMode = "table" | "cards" | "treemap";

interface ViewModeToggleProps {
  mode: ViewMode;
  onChange: (mode: ViewMode) => void;
}

const modes: { value: ViewMode; icon: typeof LayoutList; label: string }[] = [
  { value: "table", icon: LayoutList, label: "Table" },
  { value: "cards", icon: LayoutGrid, label: "Cards" },
  { value: "treemap", icon: BarChart3, label: "Treemap" },
];

export function ViewModeToggle({ mode, onChange }: ViewModeToggleProps) {
  return (
    <div className="inline-flex items-center rounded-[10px] bg-[#F0EEEA] p-0.5">
      {modes.map(({ value, icon: Icon, label }) => (
        <div key={value} className="group relative">
          <button
            type="button"
            aria-label={`${label} view`}
            onClick={() => onChange(value)}
            className={cn(
              "flex h-8 w-8 items-center justify-center rounded-[8px] transition-all duration-200",
              mode === value
                ? "bg-white text-[#1A1A19] shadow-sm"
                : "bg-transparent text-[#9B9B9B] hover:text-[#6B6B6B]"
            )}
          >
            <Icon className="h-4 w-4" />
          </button>
          <span className="pointer-events-none absolute -bottom-8 left-1/2 z-50 -translate-x-1/2 whitespace-nowrap rounded-md bg-[#1A1A19] px-2 py-1 text-[11px] text-white opacity-0 transition-opacity duration-150 group-hover:opacity-100">
            {label}
          </span>
        </div>
      ))}
    </div>
  );
}

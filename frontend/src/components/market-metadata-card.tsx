"use client";

import { useState, useCallback } from "react";
import {
  Copy,
  Check,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  Calendar,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn, formatEndDate, MONO_STYLE } from "@/lib/utils";
import type { MarketState } from "@/lib/types";

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Clipboard API not available
    }
  }, [text]);

  return (
    <Button
      variant="ghost"
      size="icon-xs"
      onClick={handleCopy}
      className="text-[#9B9B9B] hover:text-[#1A1A19]"
      title="Copy to clipboard"
    >
      {copied ? (
        <Check className="h-3 w-3 text-[#2D6A4F]" />
      ) : (
        <Copy className="h-3 w-3" />
      )}
    </Button>
  );
}

function truncateId(id: string, chars = 10): string {
  if (id.length <= chars * 2 + 3) return id;
  return `${id.slice(0, chars)}...${id.slice(-chars)}`;
}

interface MarketMetadataCardProps {
  market: MarketState;
}

export function MarketMetadataCard({ market }: MarketMetadataCardProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="rounded-2xl bg-white p-5">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center justify-between"
      >
        <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
          Market Metadata
        </h2>
        {expanded ? (
          <ChevronUp className="h-4 w-4 text-[#9B9B9B]" />
        ) : (
          <ChevronDown className="h-4 w-4 text-[#9B9B9B]" />
        )}
      </button>

      {/* Always-visible summary */}
      <div className="mt-3 flex flex-wrap items-center gap-3">
        <div className="flex items-center gap-1.5">
          <span className="text-[10px] text-[#9B9B9B]">ID:</span>
          <span
            className="text-xs text-[#1A1A19]"
            style={MONO_STYLE}
            title={market.condition_id}
          >
            {truncateId(market.condition_id)}
          </span>
          <CopyButton text={market.condition_id} />
        </div>
        {market.active ? (
          <Badge className="bg-[#DAE9E0] text-[#2D6A4F] text-[10px]">
            Active
          </Badge>
        ) : (
          <Badge className="bg-[#F0EEEA] text-[#6B6B6B] text-[10px]">
            Inactive
          </Badge>
        )}
      </div>

      {/* Expanded details */}
      {expanded && (
        <div className="mt-4 space-y-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2">
              <span className="text-[10px] text-[#9B9B9B]">Neg Risk</span>
              <p className="text-sm text-[#1A1A19]">
                {market.neg_risk ? "Yes" : "No"}
              </p>
            </div>
            <div className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2">
              <span className="text-[10px] text-[#9B9B9B]">End Date</span>
              <p className="text-sm text-[#1A1A19] inline-flex items-center gap-1">
                <Calendar className="h-3 w-3 text-[#9B9B9B]" />
                {formatEndDate(market.end_date_iso)}
              </p>
            </div>
          </div>

          {market.event_id && (
            <div className="rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2">
              <span className="text-[10px] text-[#9B9B9B]">Event ID</span>
              <div className="flex items-center gap-1.5">
                <span className="text-xs text-[#1A1A19]" style={MONO_STYLE}>
                  {truncateId(market.event_id)}
                </span>
                <CopyButton text={market.event_id} />
              </div>
            </div>
          )}

          {market.slug && (
            <a
              href={`https://polymarket.com/event/${market.slug}`}
              target="_blank"
              rel="noopener noreferrer"
              className={cn(
                "flex items-center gap-2 rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2",
                "text-sm text-[#2D6A4F] transition-colors hover:bg-[#E6E4DF]"
              )}
            >
              View on Polymarket
              <ExternalLink className="h-3.5 w-3.5" />
            </a>
          )}

          {/* Token IDs */}
          <div className="space-y-2">
            <span className="text-[10px] font-medium uppercase tracking-wider text-[#9B9B9B]">
              Token IDs
            </span>
            {market.token_ids.map((tokenId, i) => (
              <div
                key={tokenId}
                className="flex items-center justify-between rounded-[10px] border border-[#E6E4DF] bg-[#F8F7F4] px-3 py-2"
              >
                <div className="flex items-center gap-3">
                  <span className="text-[10px] text-[#9B9B9B]">
                    {market.outcomes[i] ?? `Token ${i}`}
                  </span>
                  <span
                    className="text-[10px] text-[#6B6B6B] break-all"
                    style={MONO_STYLE}
                  >
                    {truncateId(tokenId, 14)}
                  </span>
                </div>
                <CopyButton text={tokenId} />
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

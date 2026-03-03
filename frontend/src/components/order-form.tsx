"use client";

import { useState, useMemo, useCallback } from "react";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";
import { fetchApi } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import type { MarketState, ExecutionReport, Side } from "@/lib/types";

interface OrderFormProps {
  market: MarketState;
}

export function OrderForm({ market }: OrderFormProps) {
  const [selectedOutcome, setSelectedOutcome] = useState(0);
  const [side, setSide] = useState<Side>("Buy");
  const [price, setPrice] = useState("");
  const [size, setSize] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const tokenId = market.token_ids[selectedOutcome] ?? "";
  const conditionId = market.condition_id;

  // Pre-fill price from orderbook best bid/ask
  const suggestedPrice = useMemo(() => {
    const ob = market.orderbooks.find((o) => o.token_id === tokenId);
    if (!ob) return null;
    if (side === "Buy" && ob.asks[0]) return parseFloat(ob.asks[0].price);
    if (side === "Sell" && ob.bids[0]) return parseFloat(ob.bids[0].price);
    return null;
  }, [market.orderbooks, tokenId, side]);

  const priceNum = parseFloat(price) || 0;
  const sizeNum = parseFloat(size) || 0;
  const estimatedCost = priceNum * sizeNum;
  const isValid = priceNum > 0 && priceNum <= 1 && sizeNum > 0;

  const handleSubmit = useCallback(async () => {
    if (!isValid || submitting) return;
    setSubmitting(true);
    try {
      await fetchApi<ExecutionReport>("/api/order", {
        method: "POST",
        body: JSON.stringify({
          token_id: tokenId,
          condition_id: conditionId,
          side,
          price: price,
          size: size,
        }),
      });
      const outcomeName = market.outcomes[selectedOutcome] ?? "Unknown";
      toast.success(
        `Paper ${side} ${sizeNum} shares of "${outcomeName}" @ ${priceNum.toFixed(2)}`
      );
      setPrice("");
      setSize("");
    } catch (err) {
      const msg = err instanceof Error ? err.message : "Unknown error";
      toast.error(`Order failed: ${msg}`);
    } finally {
      setSubmitting(false);
    }
  }, [
    isValid,
    submitting,
    tokenId,
    conditionId,
    side,
    price,
    size,
    market.outcomes,
    selectedOutcome,
    sizeNum,
    priceNum,
  ]);

  return (
    <div className="rounded-2xl border border-[#E6E4DF] bg-white p-5">
      <h2 className="text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]">
        Place Order
      </h2>

      <div className="mt-4 space-y-4">
        {/* Outcome selector */}
        <div className="space-y-2">
          <Label className="text-[#6B6B6B]">Outcome</Label>
          <div className="flex gap-2">
            {market.outcomes.map((name, i) => (
              <button
                key={name}
                onClick={() => setSelectedOutcome(i)}
                className={cn(
                  "flex-1 rounded-[10px] border px-3 py-2 text-sm font-medium transition-all",
                  selectedOutcome === i
                    ? "border-[#2D6A4F] bg-[#DAE9E0] text-[#2D6A4F]"
                    : "border-[#E6E4DF] bg-[#F8F7F4] text-[#6B6B6B] hover:border-[#C5C3BE]"
                )}
              >
                {name}
              </button>
            ))}
          </div>
        </div>

        {/* Side toggle */}
        <div className="space-y-2">
          <Label className="text-[#6B6B6B]">Side</Label>
          <div className="flex gap-2">
            <button
              onClick={() => setSide("Buy")}
              className={cn(
                "flex-1 rounded-[10px] border px-3 py-2 text-sm font-bold transition-all",
                side === "Buy"
                  ? "border-[#2D6A4F] bg-[#2D6A4F] text-white"
                  : "border-[#E6E4DF] bg-[#F8F7F4] text-[#6B6B6B] hover:border-[#C5C3BE]"
              )}
            >
              Buy
            </button>
            <button
              onClick={() => setSide("Sell")}
              className={cn(
                "flex-1 rounded-[10px] border px-3 py-2 text-sm font-bold transition-all",
                side === "Sell"
                  ? "border-[#B44C3F] bg-[#B44C3F] text-white"
                  : "border-[#E6E4DF] bg-[#F8F7F4] text-[#6B6B6B] hover:border-[#C5C3BE]"
              )}
            >
              Sell
            </button>
          </div>
        </div>

        {/* Price input */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <Label htmlFor="order-price" className="text-[#6B6B6B]">
              Price
            </Label>
            {suggestedPrice !== null && (
              <button
                onClick={() => setPrice(suggestedPrice.toFixed(2))}
                className="text-[10px] text-[#2D6A4F] hover:underline"
              >
                Use {side === "Buy" ? "best ask" : "best bid"}: {suggestedPrice.toFixed(4)}
              </button>
            )}
          </div>
          <Input
            id="order-price"
            type="number"
            min={0.01}
            max={0.99}
            step={0.01}
            placeholder="0.01 – 0.99"
            value={price}
            onChange={(e) => setPrice(e.target.value)}
            className="border-[#E6E4DF] bg-[#F0EEEA] text-[#1A1A19]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          />
        </div>

        {/* Size input */}
        <div className="space-y-2">
          <Label htmlFor="order-size" className="text-[#6B6B6B]">
            Shares
          </Label>
          <Input
            id="order-size"
            type="number"
            min={1}
            step={1}
            placeholder="Number of shares"
            value={size}
            onChange={(e) => setSize(e.target.value)}
            className="border-[#E6E4DF] bg-[#F0EEEA] text-[#1A1A19]"
            style={{ fontFamily: "var(--font-jetbrains-mono)" }}
          />
        </div>

        {/* Estimated cost */}
        {priceNum > 0 && sizeNum > 0 && (
          <div className="flex items-center justify-between rounded-[10px] bg-[#F8F7F4] px-4 py-2.5">
            <span className="text-xs text-[#6B6B6B]">Estimated Cost</span>
            <span
              className="text-sm font-semibold text-[#1A1A19]"
              style={{ fontFamily: "var(--font-jetbrains-mono)" }}
            >
              ${estimatedCost.toFixed(2)}
            </span>
          </div>
        )}

        {/* Submit */}
        <Button
          onClick={handleSubmit}
          disabled={!isValid || submitting}
          className={cn(
            "h-11 w-full text-sm font-bold text-white",
            side === "Buy"
              ? "bg-[#2D6A4F] hover:bg-[#245840]"
              : "bg-[#B44C3F] hover:bg-[#9E3F33]"
          )}
        >
          {submitting ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : null}
          {submitting
            ? "Placing..."
            : `Place Paper ${side}`}
        </Button>
      </div>
    </div>
  );
}

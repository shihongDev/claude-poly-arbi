"use client";

import { useEffect, useRef } from "react";
import { useDashboardStore } from "@/store";
import { getWsUrl } from "@/lib/api";
import type { WsEvent } from "@/lib/types";

/** Flush interval for batching incoming WS events (ms). */
const BATCH_INTERVAL_MS = 200;

export function useWebSocket() {
  const setWsStatus = useDashboardStore((s) => s.setWsStatus);
  const handleWsEvent = useDashboardStore((s) => s.handleWsEvent);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const bufferRef = useRef<WsEvent[]>([]);
  const flushTimerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    let unmounted = false;

    function flushBuffer() {
      const events = bufferRef.current;
      if (events.length === 0) return;
      bufferRef.current = [];
      for (const event of events) {
        handleWsEvent(event);
      }
    }

    function startFlushing() {
      if (flushTimerRef.current) return;
      flushTimerRef.current = setInterval(flushBuffer, BATCH_INTERVAL_MS);
    }

    function stopFlushing() {
      if (flushTimerRef.current) {
        clearInterval(flushTimerRef.current);
        flushTimerRef.current = null;
      }
      // Drain remaining events
      flushBuffer();
    }

    function connect() {
      if (unmounted) return;
      setWsStatus("connecting");
      const ws = new WebSocket(getWsUrl());
      wsRef.current = ws;

      ws.onopen = () => {
        retryRef.current = 0;
        setWsStatus("connected");
        startFlushing();
      };

      ws.onmessage = (evt) => {
        try {
          const event: WsEvent = JSON.parse(evt.data);
          bufferRef.current.push(event);
        } catch {
          /* ignore malformed messages */
        }
      };

      ws.onclose = () => {
        stopFlushing();
        if (unmounted) return;
        setWsStatus("disconnected");
        const delay = Math.min(1000 * 2 ** retryRef.current, 30000);
        retryRef.current++;
        setTimeout(connect, delay);
      };

      ws.onerror = () => ws.close();
    }

    connect();
    return () => {
      unmounted = true;
      stopFlushing();
      wsRef.current?.close();
    };
  }, [setWsStatus, handleWsEvent]);
}

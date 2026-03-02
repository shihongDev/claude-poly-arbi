"use client";

import { useEffect, useRef } from "react";
import { useDashboardStore } from "@/store";
import { getWsUrl } from "@/lib/api";
import type { WsEvent } from "@/lib/types";

export function useWebSocket() {
  const setWsStatus = useDashboardStore((s) => s.setWsStatus);
  const handleWsEvent = useDashboardStore((s) => s.handleWsEvent);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);

  useEffect(() => {
    let unmounted = false;

    function connect() {
      if (unmounted) return;
      setWsStatus("connecting");
      const ws = new WebSocket(getWsUrl());
      wsRef.current = ws;

      ws.onopen = () => {
        retryRef.current = 0;
        setWsStatus("connected");
      };

      ws.onmessage = (evt) => {
        try {
          const event: WsEvent = JSON.parse(evt.data);
          handleWsEvent(event);
        } catch {
          /* ignore malformed messages */
        }
      };

      ws.onclose = () => {
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
      wsRef.current?.close();
    };
  }, [setWsStatus, handleWsEvent]);
}

export type WSCallback<T> = (data: T) => void;

export function createWebSocket<T>(
  path: string,
  onMessage: WSCallback<T>,
  onError?: (error: Event) => void
): { close: () => void; send: (data: unknown) => void } {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  const wsBase = process.env.NEXT_PUBLIC_WS_URL || `${protocol}//${window.location.host}/api/proxy`;
  let ws: WebSocket | null = null;
  let shouldReconnect = true;

  function connect() {
    ws = new WebSocket(`${wsBase}${path}`);

    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data) as T;
        onMessage(data);
      } catch {
        // ignore parse errors
      }
    };

    ws.onerror = (event) => {
      onError?.(event);
    };

    ws.onclose = () => {
      if (shouldReconnect) {
        setTimeout(connect, 3000);
      }
    };
  }

  connect();

  return {
    close: () => {
      shouldReconnect = false;
      ws?.close();
    },
    send: (data: unknown) => {
      if (ws?.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify(data));
      }
    },
  };
}

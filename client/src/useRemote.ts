import { useEffect, useState } from "react";
import { message, request } from "./api";

export function useRemote<T>(path: string, revision = 0) {
  const key = `${path}:${revision}`;
  const [result, setResult] = useState<{
    key: string;
    data?: T;
    error?: string;
  }>({ key: "" });
  useEffect(() => {
    const controller = new AbortController();
    void request<T>(path, { signal: controller.signal }).then(
      (data) => {
        if (!controller.signal.aborted) setResult({ key, data });
      },
      (error) => {
        if (!controller.signal.aborted)
          setResult({ key, error: message(error) });
      },
    );
    return () => controller.abort();
  }, [path, key]);
  return result.key === key
    ? { ...result, loading: false }
    : { loading: true, data: undefined, error: undefined };
}

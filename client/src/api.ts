export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

export async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const timeout = AbortSignal.timeout(12_000);
  try {
    const response = await fetch(path, {
      ...options,
      signal: options.signal
        ? AbortSignal.any([timeout, options.signal])
        : timeout,
    });
    if (!response.ok) {
      const body = await response.text();
      let detail = "";
      try {
        detail = JSON.parse(body).error ?? "";
      } catch {
        detail = body.slice(0, 200);
      }
      const messages: Record<number, string> = {
        400: "the server rejected the request parameters.",
        401: "invalid write token.",
        404: "data not found.",
        408: "the server timed out. please retry.",
        413: "batch exceeds 1 mib.",
        422: "the server rejected the json format.",
        429: "too much data or the server is busy. narrow the range and retry.",
        503: "database unavailable.",
      };
      throw new ApiError(
        response.status,
        `${messages[response.status] ?? "server error."}${detail ? ` ${detail}` : ""}`,
      );
    }
    return (await response.json()) as T;
  } catch (error) {
    if (options.signal?.aborted) throw error;
    if (error instanceof ApiError) throw error;
    if (timeout.aborted)
      throw new Error("Request timed out. Check the server and retry.", {
        cause: error,
      });
    throw new Error(
      "Cannot reach the server. Check the connection and retry.",
      { cause: error },
    );
  }
}

export const message = (error: unknown) =>
  error instanceof Error ? error.message : "Request failed.";

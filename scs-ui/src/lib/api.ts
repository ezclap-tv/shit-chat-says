type Method = "GET" | "POST" | "PUT" | "DELETE" | "PATCH";
type Headers = Record<string, string>;
type Body = Record<string, any>;
type Params = Record<string, any>;

/**
 * Helper function to send a request
 *
 * - Any search params in `endpoint` are discarded (use `params` instead)
 * - In case of a GET request, `body` is ignored
 * - The request `body` is stringified using `JSON.stringify`
 * - Default timeout is 10s
 */
function send(
  method: Method,
  uri: string,
  params: Params | null = null,
  headers: Headers | null = null,
  body: Body | null = null,
  timeout: number = 10000 /* ms */
): Promise<Response> {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), timeout);

  const url = new URL(uri);
  if (params) url.search = new URLSearchParams(params).toString();

  const init: RequestInit = {
    method: method,
    signal: controller.signal,
    mode: "cors",
  };
  if (headers) init.headers = headers;
  if (method !== "GET" && body) init.body = JSON.stringify(body);

  return fetch(url.toString(), init);
}

namespace api {
  export namespace user {
    const BASE_URL = __SCS_USER_API_URL__;

    export async function health() {
      await send("GET", BASE_URL + "/health");
    }
  }
}

export default api;

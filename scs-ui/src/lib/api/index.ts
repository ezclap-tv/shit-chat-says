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
 * - keys with values equal to `null` or `undefined` are filtered from the object (deeply)
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
  if (params) url.search = "?" + new URLSearchParams(params).toString();

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
  export namespace twitch {
    // TODO: get user profile images
  }
  export namespace user {
    const BASE_URL = __SCS_USER_API_URL__;

    export async function health() {
      const response = await send("GET", BASE_URL + "/health");
      return response.status === 200;
    }

    export namespace logs {
      export async function channels(): Promise<string[]> {
        const response = await send("GET", BASE_URL + "/v1/logs/channels");
        return await response.json();
      }

      export type Entry = {
        id: number;
        channel: string;
        chatter: string;
        sent_at: string;
        message: string;
      };
      export type LogsResponse = {
        messages: Entry[];
        cursor?: string;
      };
      export async function find(
        channel: string,
        chatter?: string | null,
        pattern?: string | null,
        cursor?: string | null,
        page_size?: number | null
      ): Promise<LogsResponse> {
        const req = {} as Params;
        if (cursor) req.cursor = cursor;
        if (chatter) req.chatter = chatter;
        if (pattern) req.pattern = pattern;
        if (page_size) req.page_size = page_size;
        const response = await send("GET", BASE_URL + `/v1/logs/${channel}`, req);
        return await response.json();
      }
    }
  }
}

// @ts-ignore
globalThis._api = api;

export default api;

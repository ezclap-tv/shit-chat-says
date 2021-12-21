import { apiStore } from "$lib/utils";
import { user as _userStore } from "$lib/auth";

type Method = "GET" | "POST" | "PUT" | "DELETE" | "PATCH";
type Headers = Record<string, string>;
type Body = Record<string, any>;
type Params = Record<string, any>;

export type Response<T> = { type: "success"; data: T } | { type: "error"; status: number; message?: string };
export type ResponseData<T> = T extends Response<infer U> ? U : never;

/**
 * Helper function to send a request
 *
 * - Any search params in `endpoint` are discarded (use `params` instead)
 * - In case of a GET request, `body` is ignored
 * - The request `body` is stringified using `JSON.stringify`
 * - Default timeout is 10s
 * - keys with values equal to `null` or `undefined` are filtered from the object (deeply)
 */
async function send<T>(
  method: Method,
  uri: string,
  params: Params | null = null,
  headers: Headers | null = null,
  body: Body | null = null,
  timeout: number = 10000 /* ms */
): Promise<Response<T>> {
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

  try {
    const response = await fetch(url.toString(), init);
    const data = await response.json();
    if (response.status < 400) {
      return {
        type: "success",
        data,
      };
    } else {
      return {
        type: "error",
        status: response.status,
        message: data.message,
      };
    }
  } catch (error) {
    return {
      type: "error",
      status: 500,
      message: "Could not reach server",
    };
  }
}

export namespace api {
  export namespace twitch {
    // TODO: get user profile images
  }
  export namespace user {
    const BASE_URL = __SCS_USER_API_URL__;

    const access = () => ({ Authorization: `Bearer ${_userStore.get()}` });

    export async function health() {
      return await send("GET", BASE_URL + "/health");
    }

    export namespace auth {
      export type TokenResponse = Response<{ token: string }>;
      export async function token(code: string, redirect_uri: string): Promise<Response<{ token: string }>> {
        return await send("POST", BASE_URL + "/token", { code, redirect_uri });
      }
    }

    export namespace logs {
      export type ChannelsResponse = Response<string[]>;
      export async function channels(): Promise<ChannelsResponse> {
        return await send("GET", BASE_URL + "/v1/logs/channels", null, access());
      }

      export type Entry = {
        id: number;
        channel: string;
        chatter: string;
        sent_at: string;
        message: string;
      };
      export type LogsResponse = Response<{
        messages: Entry[];
        cursor?: string;
      }>;
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
        return await send("GET", BASE_URL + `/v1/logs/${channel}`, req, access());
      }
    }
  }
}

// @ts-ignore
globalThis._api = api;

export namespace stores {
  export const channels = apiStore(api.user.logs.channels);
}

// @ts-ignore
globalThis._stores = stores;

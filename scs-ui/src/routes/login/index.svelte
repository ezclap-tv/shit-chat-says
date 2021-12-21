<script context="module">
  export const ssr = false;
</script>

<script lang="ts">
  import { get } from "svelte/store";
  import { twitch, user } from "$lib/auth";
  import { nonce } from "$lib/utils";
  import api from "$lib/api";
  import { goto } from "$app/navigation";
  import Loading from "$lib/components/Loading.svelte";
  import Overlay from "$lib/components/Overlay.svelte";

  let error: string | null = null;

  const CLIENT_ID = "0ncr6cfrybexz4ivgtd1kmpq0lq5an";

  const STATE_KEY = "twitch-auth-state";

  // TODO: handle errors better than this
  function restart() {
    twitch.set(null);
    user.set(null);
    delete localStorage[STATE_KEY];
    window.location.hash = "";
    window.location.search = "";
    window.location.reload();
  }

  // implicit code flow
  // https://dev.twitch.tv/docs/authentication/getting-tokens-oauth#oauth-implicit-code-flow
  const implicit = {
    step1() {
      const state = nonce();
      localStorage[STATE_KEY] = state;

      const scopes = ["user:read:email"];
      const url =
        "https://id.twitch.tv/oauth2/authorize" +
        `?client_id=${CLIENT_ID}` +
        `&redirect_uri=${globalThis.location.origin}/login` +
        `&response_type=token` +
        `&scope=${scopes.join("%20")}` +
        `&state=${state}` +
        `&force_verify=true`;

      window.location.href = url;
    },
    step2() {
      const { access_token, state } = Object.fromEntries(
        window.location.hash
          .slice(1)
          .split("&")
          .map((c) => c.split("="))
      );

      if (!access_token || state !== localStorage[STATE_KEY]) {
        return restart();
      }

      twitch.set(access_token);

      authorization.step1();
    },
  };

  // authorization code flow
  // https://dev.twitch.tv/docs/authentication/getting-tokens-oauth#oauth-authorization-code-flow
  const authorization = {
    step1() {
      const state = nonce();
      localStorage[STATE_KEY] = state;

      const scopes = ["user:read:email"];
      const redirect_uri = `${globalThis.location.origin}/login`;
      const url =
        "https://id.twitch.tv/oauth2/authorize" +
        `?client_id=${CLIENT_ID}` +
        `&redirect_uri=${redirect_uri}` +
        `&response_type=code` +
        `&scope=${scopes.join("%20")}` +
        `&state=${state}`;

      window.location.href = url;
    },
    async step2() {
      const redirect_uri = `${globalThis.location.origin}/login`;
      const params = new URL(window.location.href).searchParams;
      const code = params.get("code");
      const state = params.get("state");

      if (!code || state !== localStorage[STATE_KEY]) {
        return restart();
      }

      console.log("authorization code:", code);
      const res = await api.user.auth.token(code, redirect_uri);
      if (res.type === "success") {
        user.set(res.data.token);
        goto("/", { replaceState: true });
      } else {
        error = res.message ?? "Something went wrong...";
      }
    },
  };

  if (!get(twitch)) {
    if (window.location.hash) implicit.step2();
    else implicit.step1();
  } else if (!get(user)) {
    if (window.location.search) authorization.step2();
    else authorization.step1();
  } else {
    goto("/", { replaceState: true });
  }
</script>

<Overlay transparent>
  {#if !error}
    <Loading />
  {:else}
    <span>{error}</span>
  {/if}
</Overlay>

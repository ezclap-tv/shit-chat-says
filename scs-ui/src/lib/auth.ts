import { browser } from "$app/env";
import { localStore, type LocalStore } from "./utils";

export let twitch: LocalStore<string>;
export let user: LocalStore<string>;

if (browser) {
  twitch = localStore<string>("twitch-auth");
  user = localStore<string>("user-auth");

  // @ts-ignore
  window._auth = { twitch, user };

  const hasTokens = "twitch-auth" in localStorage && "user-auth" in localStorage;
  if (!hasTokens && !window.location.pathname.includes("login")) {
    window.location.replace("/login");
  }
}

import { browser } from "$app/env";
import { localStore, type LocalStore } from "./utils";

export let twitch: LocalStore<string>;
export let user: LocalStore<string>;

if (browser) {
  const token = localStorage.getItem("auth");
  if (!token && !window.location.pathname.includes("login")) {
    window.location.replace("/login");
  }

  twitch = localStore<string>("twitch-auth");
  user = localStore<string>("user-auth");

  // @ts-ignore
  window._auth = { twitch, user };
}

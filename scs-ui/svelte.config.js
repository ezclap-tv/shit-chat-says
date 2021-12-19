import adapter from "@sveltejs/adapter-static";
import preprocess from "svelte-preprocess";
import { replaceCodePlugin as replace } from "vite-plugin-replace";

process.env.SCS_USER_API_URL ??= "http://localhost:8080";
process.env.SCS_MANAGE_API_URL ??= "http://localhost:7191";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: preprocess(),
  kit: {
    adapter: adapter({
      pages: "build",
      assets: "build",
      fallback: "index.html",
    }),
    ssr: false,
    target: "#svelte",
    vite: {
      plugins: [
        replace({
          replacements: [
            { from: "__SCS_USER_API_URL__", to: process.env.SCS_USER_API_URL },
            { from: "__SCS_MANAGE_API_URL__", to: process.env.SCS_MANAGE_API_URL },
          ],
        }),
      ],
    },
  },
};

export default config;

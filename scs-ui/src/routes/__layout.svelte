<script lang="ts" context="module">
  export const ssr = false;
</script>

<script lang="ts">
  import "$lib/auth";
  import "$lib/api";
  import { page } from "$app/stores";
  import Navigation from "$lib/components/Navigation.svelte";
</script>

{#if !$page.path.includes("login")}
  <Navigation />
{/if}
<main>
  <slot />
</main>

<style lang="scss">
  :global(*) {
    box-sizing: border-box;
  }

  :global(:not(pre)) {
    font-family: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", "Liberation Sans",
      sans-serif, "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji";
  }

  :global(html, body, div#svelte) {
    background: #fff;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    margin: 0 !important;
    padding: 0 !important;
  }

  :global(main) {
    max-width: 60%;
    width: 100%;
    margin: auto;

    padding: 32px 16px;
  }

  :global(:root) {
    --primary: #2081c3;
    --primary-rgb: 32, 129, 195;
    --secondary: #255b80;
    --secondary-rgb: 37, 91, 128;
  }
</style>

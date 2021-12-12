<script lang="ts">
  import Nav from "$lib/nav.svelte";
</script>

<Nav />
<main>
  <slot />
</main>

<style lang="scss">
  :global(*) {
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

  main {
    max-width: 60%;
    width: 100%;
    margin: auto;

    padding: 32px 16px;
  }
</style>

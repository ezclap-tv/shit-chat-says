<script lang="ts">
  import Navigation from "$lib/components/Navigation.svelte";
</script>

<Navigation />
<main>
  <slot />
</main>

<style lang="scss">
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
</style>

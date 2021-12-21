<script context="module">
  export const ssr = false;
</script>

<script lang="ts">
  import Loading from "$lib/components/Loading.svelte";
  import Overlay from "$lib/components/Overlay.svelte";
  import { stores } from "$lib/api";
  const channels = stores.channels;
  channels.update();
</script>

{#await $channels}
  <Overlay anchor="top" transparent><Loading /></Overlay>
{:then channels}
  <div class="grid">
    {#each channels as channel}
      <a href={`/logs/${channel}`}>
        <span>{channel}</span>
      </a>
    {/each}
  </div>
{:catch e}
  <Overlay anchor="top" transparent>
    <span>Failed to load channels: {e}</span>
  </Overlay>
{/await}

<style lang="scss">
  .grid {
    display: flex;
    flex-wrap: wrap;

    > a {
      /* 4 per row, 4px * 2 = margin on both sides */
      width: calc(100% / 4 - 4px * 2);
      height: 48px;
      margin: 4px;

      display: flex;
      > * {
        margin: auto;
      }

      text-decoration: none;
      border-radius: 8px;
      border: 1px solid;
      color: var(--secondary);
      &:visited {
        color: var(--secondary);
      }
      &:hover {
        color: var(--primary);
      }
    }
  }
</style>

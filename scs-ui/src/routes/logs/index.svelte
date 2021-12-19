<script lang="ts">
  import { channels } from "$lib/api/stores";

  channels.update();
</script>

<span>Logs</span>
{#await $channels}
  <span>(spinner)</span>
{:then channels}
  <ul>
    {#each channels as channel}
      <li>{channel} <a href={`/logs/${channel}`}>show</a></li>
    {/each}
  </ul>
{:catch e}
  <span>Failed to load channels: {e}</span>
{/await}

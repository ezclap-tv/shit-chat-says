<script context="module">
  export const ssr = false;
</script>

<script lang="ts">
  import api from "$lib/api";
  import { stagger } from "$lib/utils";
  import { page } from "$app/stores";
  import Loading from "$lib/components/Loading.svelte";
  import Overlay from "$lib/components/Overlay.svelte";
  import InfiniteScroll from "$lib/components/InfiniteScroll.svelte";
  import { onMount } from "svelte";

  const channel = $page.params.channel;
  let chatter: string = "";
  let pattern: string = "";
  let page_size: number = 100;

  let logs: api.user.logs.Entry[] = [];
  let cursor: string | undefined;
  let isAtEnd = false;
  let scroll: InfiniteScroll | undefined;

  let loading: "full" | "page" | "none" = "full";

  // full reload - when any of the inputs change
  const loadFull = stagger(async () => {
    const data = await api.user.logs.find(channel, chatter, pattern, null, page_size);
    logs = data.messages; // full fetch = discard all messages
    cursor = data.cursor;
    isAtEnd = !data.cursor; // no cursor in response = no more messages
    scroll?.reset();
    loading = "none";
  }, 1000);

  // because `loadFull` is staggered, we have to set `loading` state to `full` in a separate function
  const input = () => {
    loading = "full";
    loadFull();
  };

  // load next page - when user scrolls to bottom of messages
  const loadNext = async () => {
    if (isAtEnd || loading !== "none") {
      scroll?.reset();
      return;
    }
    loading = "page";
    const data = await api.user.logs.find(channel, chatter, pattern, cursor, page_size);
    logs = [...logs, ...data.messages]; // page fetch = insert messages
    cursor = data.cursor;
    isAtEnd = !data.cursor; // no cursor in response = no more messages
    scroll?.reset();
    loading = "none";
  };

  onMount(() => loadFull());
</script>

<span>Logs for {channel} <a href="/logs">back</a></span>
<br />

<input type="text" bind:value={chatter} on:input={input} placeholder="chatter" />
<br />

<input type="text" bind:value={pattern} on:input={input} placeholder="pattern" />
<br />

<input type="range" bind:value={page_size} on:input={input} min={100} max={1000} step={100} />
<span>{page_size}</span>
<br />

<div class="logs">
  {#if loading === "full"}
    <Overlay anchor="top"><Loading /></Overlay>
  {/if}
  {#each logs as line (line.id)}
    <div>{line.chatter}: {line.message}</div>
  {/each}
  {#if loading === "page"}
    <div><Loading /></div>
  {/if}
  <InfiniteScroll bind:this={scroll} on:next={loadNext} />
</div>

<style lang="scss">
  .logs {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100%;
    min-height: 120px;

    background-color: #efeff1;
    color: #18181b;

    > div {
      font-family: Inter, Roobert, "Helvetica Neue", Helvetica, Arial, sans-serif;
      font-size: 13px;
      padding: 0.5em 2em;
    }
  }
</style>

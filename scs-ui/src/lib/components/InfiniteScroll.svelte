<script lang="ts">
  import { onMount, createEventDispatcher } from "svelte";

  export let threshold = 0;

  /**
   * Tell the `InfiniteScroll` component that it can dispatch the `next` event again
   *
   * Call this after you're done responding to the event (e.g. after data loading finishes)
   */
  export const reset = () => (shouldLoadMore = true);

  const dispatch = createEventDispatcher<{ next: undefined }>();
  let shouldLoadMore = true;
  let self: HTMLDivElement;

  const onScroll = (e: Event) => {
    const isVisible = window.innerHeight - self.getBoundingClientRect().top > threshold;
    if (isVisible) {
      if (shouldLoadMore) {
        dispatch("next");
      }
      shouldLoadMore = false;
    }
  };

  onMount(() => {
    window.addEventListener("scroll", onScroll);
    window.addEventListener("resize", onScroll);

    return () => {
      window.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
    };
  });
</script>

<div bind:this={self} style="width:0px;height:0px" />

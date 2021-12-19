<script lang="ts">
  export let anchor: "top" | "bottom" | "left" | "right" | "center" = "center";
</script>

<div class={anchor}>
  <slot />
</div>

<style lang="scss">
  div {
    pointer-events: none;
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    background-color: rgba(0, 0, 0, 0.3);

    display: flex;

    &.center:global(> *) {
      margin: auto;
    }
    &.top:global(> *) {
      margin: 16px auto auto auto;
    }
    &.right:global(> *) {
      margin: auto 16px auto auto;
    }
    &.bottom:global(> *) {
      margin: auto auto 16px auto;
    }
    &.left:global(> *) {
      margin: auto auto auto 16px;
    }
  }
</style>

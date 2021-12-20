<script lang="ts" context="module">
  export type Route = {
    name: string;
    href: string;
  };
</script>

<script lang="ts">
  import Menu from "material-icons/Menu.svelte";
  import { page } from "$app/stores";

  const routes: Route[] = [
    { name: "Logs", href: "/logs" },
    { name: "Models", href: "/models" },
    { name: "Admin", href: "/admin" },
  ];

  let open = false;

  function toggle() {
    open = !open;
  }
</script>

<nav class:open>
  <ul>
    {#each routes as { name, href }}
      <li><a class:current={$page.path === href} {href}>{name}</a></li>
    {/each}
  </ul>
  <div class="toggle" class:open on:click={toggle} />
  <div class="widget" class:open on:click={toggle}>
    <Menu />
  </div>
</nav>

<style lang="scss">
  nav {
    /* nav is floating and static */
    position: fixed;
    top: 0;
    height: 100%;
    width: 200px;
    padding: 16px 0 0 16px;
    /* show/hide animation */
    transition: left 0.15s ease-in-out;
    left: -200px;
    &.open {
      left: 0;
    }
    background-color: rgba(255, 255, 255, 0.8);
    box-shadow: 2px 0px 4px rgba(0, 0, 0, 0.6);
    backdrop-filter: blur(4px);
    > ul {
      list-style: none;
      margin: 0;
      padding: 0;
      > li {
        padding: 0px;
        /* on hover is for the whole list item, not just the text */
        > a {
          display: block;
          width: 100%;
          height: 100%;
          margin: 0 0 0 8px;
          padding: 8px 8px;
          text-decoration: none;
          font-size: 20px;
          color: black;
          &:visited {
            color: black;
          }
          &:hover {
            color: #2081c3;
            border-left: 4px solid #255b80;
          }
          &.current {
            border-left: 4px solid #2081c3;
          }
        }
      }
    }
    > .toggle {
      position: absolute;
      top: 0;
      left: 100%;
      height: 100%;
      width: 32px;
      cursor: pointer;
      transition: background-color 0.15s ease-in-out, box-shadow 0.15s ease-in-out;
      background-color: none;
      backdrop-filter: blur(4px);
      &:hover {
        background-color: rgba(0, 0, 0, 0.5);
        box-shadow: 2px 0px 4px rgba(0, 0, 0, 0.6);
      }
    }
    > .widget {
      position: absolute;
      top: 16px;
      left: calc(100% + 16px);
      width: 48px;
      height: 48px;
      border-radius: 100%;
      background-color: none;
      cursor: pointer;
      transition: background-color 0.15s ease-in-out, box-shadow 0.15s ease-in-out, opacity 0.15s ease-in-out;
      pointer-events: all;
      opacity: 1;
      &:hover {
        background-color: rgba(0, 0, 0, 0.3);
        box-shadow: 2px 0px 4px 2px rgba(0, 0, 0, 0.1);
      }
      &.open {
        pointer-events: none;
        opacity: 0;
      }
      display: flex;
      justify-content: center;
      align-items: center;
    }
  }
</style>

# scs-ui

SCS UI built with [SvelteKit](https://kit.svelte.dev/)

### Local development

```
$ yarn
$ yarn dev
```

If you want to be able to do anything, ensure that the `scs-user-api` and/or `scs-manage-api` services are running.
To override their base URLs, use the `SCS_USER_API_URL` and `SCS_MANAGE_API_URL` environment variables:

```
$ SCS_USER_API_URL=https://localhost:8000 SCS_MANAGE_API_URL=https://localhost:8001 yarn dev
```
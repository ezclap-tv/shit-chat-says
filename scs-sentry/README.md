# Sentry Integration

A library for initializing Sentry from the ability to either skip initialize or completely disable Sentry at build time.

- If the `SCS_SENTRY_DSN` env variable is set, initializes Sentry with the DSN, the value of `sentry::release_name!()`
  - If the variable is not set, skips initialization and does nothing.
  - Additionally, if `SCS_SENTRY_ENV` is set, sets `SENTRY_ENVIRONMENT` to `{binary_or_crate_name}-{SCS_SENTRY_ENV}`.

## Use Sentry

1. Initialize at the start of the program

```rust
fn main() {
  // Expands to `let _guard = scs_sentry::_init_from_env(...);`
  scs_sentry::from_env!();
}
```

2. If needed, use the `use_sentry` macro to access `sentry` in a manner that allows your code to compile with Sentry disabled.

```rust
scs_sentry::use_sentry! {
  // The sentry library is accessible here.
  sentry::do_stuff();
};
```

## Disable Sentry

To completely disable Sentry at compile time for all crates in the project, modify `Cargo.toml` and set `default` to an empty list `[]`.

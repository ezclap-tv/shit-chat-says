#[cfg(feature = "sentry")]
mod sentry_impl {
  pub use sentry;
  pub use sentry::ClientInitGuard;

  #[macro_export]
  macro_rules! sentry_options {
    ($($key:ident : $value:expr),*) => {{
      $crate::sentry::ClientOptions {
        $(
          $key: $value,
        )*
        ..Default::default()
      }
    }};
  }

  #[macro_export]
  macro_rules! from_env {
    ($($key:ident : $value:expr),*) => {
      let _guard = $crate::_init_from_env(
        option_env!("CARGO_BIN_NAME").unwrap_or(env!("CARGO_PKG_NAME")),
        $crate::sentry::ClientOptions {
          release: $crate::sentry::release_name!(),
          ..$crate::sentry_options! {
            $(
              $key: $value
            ),*
          }
      });
    };
  }

  #[macro_export]
  macro_rules! use_sentry {
    ($($tok:tt)*) => {{
      use $crate::sentry;
      $($tok)*
    }};
  }

  pub fn _init(options: sentry::ClientOptions) -> sentry::ClientInitGuard {
    sentry::init(options)
  }

  #[must_use]
  pub fn _init_from_env(pkg_name: &str, options: sentry::ClientOptions) -> Option<sentry::ClientInitGuard> {
    if let Ok(env) = std::env::var("SCS_SENTRY_ENV") {
      std::env::set_var("SENTRY_ENVIRONMENT", format!("{pkg_name}-{env}"));
    }

    if let Ok(dsn) = std::env::var("SCS_SENTRY_DSN") {
      Some(_init((dsn, options).into()))
    } else {
      None
    }
  }
}

#[cfg(not(feature = "sentry"))]
mod sentry_impl {
  pub mod sentry {}
  pub type ClientInitGuard = ();
  pub type ClientOptions = ();

  #[macro_export]
  macro_rules! sentry_options {
    ($($key:ident : $value:expr),*) => {{}};
  }

  #[macro_export]
  macro_rules! from_env {
    ($($key:ident : $value:expr),*) => {
      let _guard = None::<$crate::ClientInitGuard>;
    };
  }

  #[macro_export]
  macro_rules! use_sentry {
    ($($tok:tt)*) => {{}};
  }

  pub fn _init(_: ClientOptions) -> ClientInitGuard {
    ClientInitGuard::default()
  }

  pub fn _init_from_env(_: &str, _: ClientOptions) -> Option<ClientInitGuard> {
    None
  }
}

pub use sentry_impl::*;

pub fn test() {
  crate::from_env!();
}

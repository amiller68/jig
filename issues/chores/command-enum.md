# [Title]

**Status:** Planned

## Objective

Other CLIs I maintain have a nice little Op macro which handles

- command registration
- context passing
- argument formation
- and execution

through one Trait and Macro:

```rust
use std::error::Error;
use std::path::PathBuf;

use url::Url;

use jax_daemon::http_server::api::client::{ApiClient, ApiError};
use jax_daemon::state::AppState;

#[derive(Clone)]
pub struct OpContext {
    pub some_thing: Thing;
    pub config_path: Option<PathBuf>,
    // etc etc
}

impl OpContext {
    /// Create context with custom remote URL and optional config path
    pub fn new(...) -> Result<Self, ApiError> {
        Ok(Self {
          ...
        })
    }
}

#[async_trait::async_trait]
pub trait Op: Send + Sync {
    type Error: Error + Send + Sync + 'static;
    type Output;

    async fn execute(&self, ctx: &OpContext) -> Result<Self::Output, Self::Error>;
}

#[macro_export]
macro_rules! command_enum {
    ($(($variant:ident, $type:ty)),* $(,)?) => {
        #[derive(Subcommand, Debug, Clone)]
        pub enum Command {
            $($variant($type),)*
        }

        #[derive(Debug)]
        pub enum OpOutput {
            $($variant(<$type as $crate::cli::op::Op>::Output),)*
        }

        #[derive(Debug, thiserror::Error)]
        pub enum OpError {
            $(
                #[error(transparent)]
                $variant(<$type as $crate::cli::op::Op>::Error),
            )*
        }

        #[async_trait::async_trait]
        impl $crate::cli::op::Op for Command {
            type Output = OpOutput;
            type Error = OpError;

            async fn execute(&self, ctx: &$crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
                match self {
                    $(
                        Command::$variant(op) => {
                            op.execute(ctx).await
                                .map(OpOutput::$variant)
                                .map_err(OpError::$variant)
                        },
                    )*
                }
            }
        }

        impl std::fmt::Display for OpOutput {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(
                        OpOutput::$variant(output) => write!(f, "{}", output),
                    )*
                }
            }
        }
    };
}
```

```

```

which can streamline defining operations:

```rust
use clap::Args;

use common::build_info;

#[derive(Args, Debug, Clone)]
pub struct Version;

#[derive(Debug, thiserror::Error)]
pub enum VersionError {
    #[error("Version operation failed: {0}")]
    Failed(String),
}

#[async_trait::async_trait]
impl crate::cli::op::Op for Version {
    type Error = VersionError;
    type Output = String;

    async fn execute(&self, _ctx: &crate::cli::op::OpContext) -> Result<Self::Output, Self::Error> {
        Ok(build_info!().to_string())
    }
}
```

```

```

registering commands within the cli becomes trivial:

```rust
command_enum! {
    ...
    (Version, Version),
    ...
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Resolve remote URL: explicit flag > config api_port > hardcoded 5001
    let remote = cli::op::resolve_remote(args.remote, args.config_path.clone());

    // Build context - always has API client initialized
    let ctx = match cli::op::OpContext::new(remote, args.config_path) {
        Ok(ctx) => ctx,
        Err(e) => {
            eprintln!("Error: Failed to create API client: {}", e);
            std::process::exit(1);
        }
    };

    match args.command.execute(&ctx).await {
        Ok(output) => {
            println!("{}", output);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}
```

```

```

## Implementation

1. Implement the Trait without context
2. Update all the commands to use the new trait
3. Pass over commands and integrate them into the new macro for generating the cli
4. Audit the commands for re-used context that can now be shared
5. Implement shared context + simplify commands
6. Cleanup

## Acceptance Criteria

- [ ] Commands work exactly as they had before
- [ ] We have net code deletion

## Verification

How to test this works.

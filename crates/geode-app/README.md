# geode-app

The shared application spine for the GEODE-FEM binaries (Epic #398). It
provides the clap wiring, a uniform `Result → ExitCode` lifecycle, a few
reusable argument groups and one attach point for a future logging backend.
The benchmark drivers in [`examples/`](../../examples/) and the `geode` CLI use
it. Its only dependencies are `clap` and `thiserror`, which keeps the public
surface small.

| Module | Contents |
|---|---|
| [`src/runner.rs`](src/runner.rs) | The `App` trait and `main`, which parse arguments, run, and map the result to an exit code |
| [`src/args.rs`](src/args.rs) | The `OutputDir` (`--out-dir`) and `Verbosity` (`-v` / `-q`) argument groups |
| [`src/lifecycle.rs`](src/lifecycle.rs) | `init_observability`, the logging seam; a no-op until a logging backend is chosen |

Implement `App` on the top-level `clap::Parser` struct and make `main` a
one-liner:

```rust
use std::process::ExitCode;
use clap::Parser;
use geode_app::{App, OutputDir, Verbosity};

#[derive(Parser)]
struct MyArgs {
    #[command(flatten)]
    out: OutputDir,
    #[command(flatten)]
    verbose: Verbosity,
}

impl App for MyArgs {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.out.resolve()?; // creates and returns the artifact dir
        println!("writing artifacts to {}", dir.display());
        Ok(())
    }
    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

fn main() -> ExitCode {
    geode_app::main::<MyArgs>()
}
```

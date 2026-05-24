use std::process::ExitCode;

fn main() -> ExitCode {
    println!("geode-fem {}", env!("CARGO_PKG_VERSION"));

    match geode_core::smoke_add() {
        Ok(info) => {
            println!("  backend: {}", info.backend);
            println!("  device:  {}", info.device_label);
            println!("  smoke:   ok");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("  smoke:   FAILED: {e}");
            ExitCode::from(1)
        }
    }
}

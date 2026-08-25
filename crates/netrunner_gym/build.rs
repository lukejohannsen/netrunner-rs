//! Some Python installations (this repo's dev sandbox included) ship
//! `libpython3.x.so` in `sysconfig`'s `LIBPL` directory
//! (`.../config-3.x-<arch>/`) rather than the default library search path
//! `-lpython3.x` looks in — normally a `python3-dev`/`libpython3-dev`
//! package symlinks it into the default path, but that package isn't
//! guaranteed to be present. Point the linker at `LIBPL` directly so the
//! default (embedding, non-`extension-module`) build links successfully
//! without requiring that package. Harmless if unused: with the
//! `extension-module` feature enabled, PyO3 doesn't link `libpython` at
//! all, so this extra search path is simply never consulted.

use std::process::Command;

fn main() {
    let python = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_string());
    let output = Command::new(&python).args(["-c", "import sysconfig; print(sysconfig.get_config_var('LIBPL') or '')"]).output();

    if let Ok(output) = output
        && output.status.success()
    {
        let libpl = String::from_utf8_lossy(&output.stdout);
        let libpl = libpl.trim();
        if !libpl.is_empty() {
            println!("cargo:rustc-link-search=native={libpl}");
        }
    }
}

//! Native startup registration without a Tauri runtime.
use crate::{
    common::{err, Result},
    Store,
};
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

fn launcher() -> Result<AutoLaunch> {
    let executable = std::env::current_exe().map_err(err)?;
    #[cfg(target_os = "macos")]
    let executable = executable.canonicalize().map_err(err)?;
    let mut builder = AutoLaunchBuilder::new();
    // The original productName is TP. Keep the same registry / LaunchAgent identity.
    builder
        .set_app_name("TP")
        .set_app_path(&executable.to_string_lossy());
    #[cfg(target_os = "macos")]
    builder.set_use_launch_agent(true);
    builder.build().map_err(err)
}

fn apply(launcher: &AutoLaunch, enabled: bool) -> Result<()> {
    if enabled {
        launcher.enable().map_err(err)
    } else if launcher.is_enabled().map_err(err)? {
        launcher.disable().map_err(err)
    } else {
        // Windows reports an error when deleting an already missing startup value.
        Ok(())
    }
}

pub fn enabled() -> Result<bool> {
    launcher()?.is_enabled().map_err(err)
}

pub fn set(store: &Store, enabled: bool) -> Result<()> {
    let launcher = launcher()?;
    let _data = store.read_data()?;
    let conn = store.db.lock().map_err(err)?;
    let current = launcher.is_enabled().map_err(err)?;
    tp_core::preferences::save_autostart(&conn, enabled, current, |value| apply(&launcher, value))
}

pub fn restore(store: &Store) -> Result<()> {
    let launcher = launcher()?;
    let _data = store.read_data()?;
    let conn = store.db.lock().map_err(err)?;
    let current = launcher.is_enabled().map_err(err)?;
    tp_core::preferences::restore_autostart(&conn, current, |value| apply(&launcher, value))
}

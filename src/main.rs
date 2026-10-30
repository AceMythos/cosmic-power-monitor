mod app;
mod battery;
mod config;
mod localize;

use crate::app::PowerMonitor;
use localize::localize;

fn main() -> cosmic::iced::Result {
    localize();

    // libcosmic's applet module logs the panel height at info level on every
    // layout pass, which spams the journal. Keep our own info logs, drop that
    // target's noise.
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,cosmic::applet=warn"),
    )
    .init();

    cosmic::applet::run::<PowerMonitor>(())?;

    Ok(())
}

mod app;
mod battery;
mod config;
mod localize;

use crate::app::PowerMonitor;
use localize::localize;

fn main() -> cosmic::iced::Result {
    localize();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    cosmic::applet::run::<PowerMonitor>(())?;

    Ok(())
}

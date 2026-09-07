mod app;
mod battery;
mod config;

use crate::app::PowerMonitor;

fn main() -> cosmic::iced::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    cosmic::applet::run::<PowerMonitor>(())?;

    Ok(())
}

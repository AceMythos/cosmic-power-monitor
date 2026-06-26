mod app;
mod battery;

use crate::app::PowerMonitor;

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<PowerMonitor>(())?;

    Ok(())
}

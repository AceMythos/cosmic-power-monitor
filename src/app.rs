use cosmic::app::Core;
use cosmic::cosmic_config::{Config, CosmicConfigEntry};
use cosmic::iced::core::animation::{Animation, Easing};
use cosmic::iced::mouse;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Limits, Subscription};
use cosmic::widget::{
    button, canvas, column, container, divider, row, segmented_button, segmented_control, text,
};
use cosmic::{Action, Element, Task, Theme};
use cosmic::iced::Color;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::battery;
use crate::config::{PanelDisplay, PowerMonitorConfig};

const ID: &str = "io.github.AceMythos.cosmic-ext-applet-power-monitor";

/// Poll cadence, read fresh by the subscription on every tick so a mode change
/// takes effect without tearing down and rebuilding the subscription.
static POLL_MS: AtomicU64 = AtomicU64::new(FAST_POLL_MS);

/// Rendered when the selected mode has no reading to show, so the panel entry
/// stays wide enough to click.
const EMPTY_LABEL: &str = "—";

/// Watts move fast enough to need a tight loop; charge level does not.
const FAST_POLL_MS: u64 = 250;
const SLOW_POLL_MS: u64 = 5_000;

pub struct PowerMonitor {
    core: Core,
    popup: Option<Id>,
    watts: f64,
    display_watts: Animation<f32>,
    percentage: f64,
    status: String,
    time_to_empty: i64,
    time_to_full: i64,
    energy: f64,
    energy_full: f64,
    no_battery: bool,
    config: PowerMonitorConfig,
    config_handler: Option<Config>,
    display_modes: segmented_button::SingleSelectModel,
    batteries: Vec<battery::BatteryInfo>,
}

impl Default for PowerMonitor {
    fn default() -> Self {
        Self {
            core: Core::default(),
            popup: None,
            watts: 0.0,
            display_watts: Animation::new(0.0)
                .duration(Duration::from_millis(200))
                .easing(Easing::EaseOutCubic),
            percentage: 0.0,
            status: String::new(),
            time_to_empty: 0,
            time_to_full: 0,
            energy: 0.0,
            energy_full: 0.0,
            no_battery: false,
            config: PowerMonitorConfig::default(),
            config_handler: None,
            display_modes: segmented_button::SingleSelectModel::default(),
            batteries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Update(battery::BatteryData),
    NoBattery,
    DisplayModeSelected(segmented_button::Entity),
    ConfigUpdated(PowerMonitorConfig),
}

impl PowerMonitor {
    fn format_watts(w: f64) -> String {
        if w >= 1.0 {
            format!("{:.1}W", w)
        } else if w >= 0.1 {
            format!("{:.2}W", w)
        } else {
            format!("{:.3}W", w)
        }
    }

    fn format_power_string(&self, watts: f64, with_time: bool) -> String {
        if self.no_battery {
            return String::new();
        }
        if matches!(self.status.as_str(), "Full" | "Fully Charged") {
            return "✓ Full".to_string();
        }
        if watts <= 0.0 {
            if self.status == "Charging" || self.status == "Not charging" {
                return "Stopped".to_string();
            }
            return String::new();
        }
        let sign = if self.status == "Charging" { "+" } else { "-" };
        let time = match self.status.as_str() {
            "Charging" if with_time && self.time_to_full > 0 => {
                format!("({})", Self::format_time(self.time_to_full))
            }
            "Discharging" if with_time && self.time_to_empty > 0 => {
                format!("({})", Self::format_time(self.time_to_empty))
            }
            _ => String::new(),
        };
        format!("{}{}{}", sign, Self::format_watts(watts), time)
    }

    fn format_percentage(&self) -> String {
        if self.no_battery {
            return String::new();
        }
        format!("{:.0}%", self.percentage)
    }

    /// The panel label, per the configured display mode.
    fn format_panel_string(&self, watts: f64) -> String {
        let label = match self.config.panel_display {
            PanelDisplay::Percentage => self.format_percentage(),
            PanelDisplay::Power => self.format_power_string(watts, true),
            PanelDisplay::Both => {
                let percentage = self.format_percentage();
                // No time estimate here, or the panel entry gets very wide.
                let power = self.format_power_string(watts, false);
                match (percentage.is_empty(), power.is_empty()) {
                    (_, true) => percentage,
                    (true, _) => power,
                    _ => format!("{}  {}", percentage, power),
                }
            }
        };

        // A mode with nothing to report would otherwise render an empty button,
        // leaving only a sliver of padding to click to reach the popup. A battery
        // idling at a charge threshold reports 0W indefinitely, so this is not a
        // momentary state.
        if label.is_empty() {
            EMPTY_LABEL.to_string()
        } else {
            label
        }
    }

    /// Charge level only needs a lazy poll, but the popup shows live watts, so
    /// stay fast whenever it is open.
    fn poll_interval_ms(&self) -> u64 {
        if self.popup.is_some() || self.config.panel_display != PanelDisplay::Percentage {
            FAST_POLL_MS
        } else {
            SLOW_POLL_MS
        }
    }

    fn apply_poll_interval(&self) {
        POLL_MS.store(self.poll_interval_ms(), Ordering::Relaxed);
    }

    /// Point the segmented control at whatever the config currently says.
    fn sync_display_modes(&mut self) {
        let active = self
            .display_modes
            .iter()
            .find(|entity| {
                self.display_modes.data::<PanelDisplay>(*entity) == Some(&self.config.panel_display)
            });
        if let Some(entity) = active {
            self.display_modes.activate(entity);
        }
    }

    fn format_time(seconds: i64) -> String {
        if seconds <= 0 {
            return String::new();
        }
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m", minutes)
        } else {
            format!("{}s", seconds)
        }
    }
}

fn battery_fill_color(pct: f32) -> Color {
    if pct > 0.6 {
        Color::from_rgb(0.3, 0.8, 0.3)
    } else if pct > 0.2 {
        Color::from_rgb(0.9, 0.6, 0.1)
    } else {
        Color::from_rgb(0.8, 0.2, 0.2)
    }
}

struct BatteryBar {
    percentage: f32,
}

impl canvas::Program<Message, cosmic::Theme> for BatteryBar {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::iced::Renderer,
        _theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let width = bounds.width;
        let height = bounds.height;
        let fill_width = width * self.percentage;

        let track_color = Color::from_rgba(0.5, 0.5, 0.5, 0.15);
        frame.fill_rectangle(
            cosmic::iced::Point::new(0.0, 0.0),
            cosmic::iced::Size::new(width, height),
            track_color,
        );

        let fill_color = battery_fill_color(self.percentage);

        if fill_width > 0.0 {
            frame.fill_rectangle(
                cosmic::iced::Point::new(0.0, 0.0),
                cosmic::iced::Size::new(fill_width, height),
                fill_color,
            );
        }

        vec![frame.into_geometry()]
    }
}

struct BatteryIcon {
    percentage: f32,
    charging: bool,
}

impl canvas::Program<Message, cosmic::Theme> for BatteryIcon {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::iced::Renderer,
        _theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let body = cosmic::iced::Rectangle::new(
            cosmic::iced::Point::new(1.0, 1.0),
            cosmic::iced::Size::new(44.0, 30.0),
        );
        let outline = Color::from_rgba(0.5, 0.5, 0.5, 0.5);

        let fill_color = battery_fill_color(self.percentage);
        let fill_width = (body.width - 8.0) * self.percentage.clamp(0.0, 1.0);
        if fill_width > 0.0 {
            frame.fill_rectangle(
                cosmic::iced::Point::new(4.0, 4.0),
                cosmic::iced::Size::new(fill_width, body.height - 8.0),
                fill_color,
            );
        }

        frame.fill_rectangle(
            cosmic::iced::Point::new(45.0, 11.0),
            cosmic::iced::Size::new(3.0, 10.0),
            outline,
        );

        if self.charging && self.percentage < 1.0 {
            let bolt = canvas::Path::new(|b| {
                b.move_to(cosmic::iced::Point::new(26.0, 5.0));
                b.line_to(cosmic::iced::Point::new(19.0, 16.0));
                b.line_to(cosmic::iced::Point::new(23.0, 16.0));
                b.line_to(cosmic::iced::Point::new(15.0, 27.0));
                b.line_to(cosmic::iced::Point::new(26.0, 17.0));
                b.line_to(cosmic::iced::Point::new(22.0, 17.0));
                b.close();
            });
            frame.fill(&bolt, Color::from_rgba(1.0, 1.0, 1.0, 0.92));
        }

        let body_path = canvas::Path::new(|b| {
            b.rounded_rectangle(
                body.position(),
                body.size(),
                cosmic::iced::border::Radius::from(4.0),
            )
        });
        frame.stroke(
            &body_path,
            canvas::Stroke::default().with_width(2.0).with_color(outline),
        );

        vec![frame.into_geometry()]
    }
}

impl cosmic::Application for PowerMonitor {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        log::info!("Starting Power Monitor");

        let config_handler = Config::new(ID, PowerMonitorConfig::VERSION)
            .inspect_err(|e| log::warn!("failed to open config, using defaults: {e}"))
            .ok();
        let config = config_handler
            .as_ref()
            .map(|handler| {
                PowerMonitorConfig::get_entry(handler).unwrap_or_else(|(errs, config)| {
                    for err in errs {
                        log::warn!("config load error: {err}");
                    }
                    config
                })
            })
            .unwrap_or_default();

        let mut display_modes = segmented_button::SingleSelectModel::default();
        for mode in PanelDisplay::ALL {
            display_modes.insert().text(mode.label()).data(mode);
        }

        let mut app = PowerMonitor {
            core,
            config,
            config_handler,
            display_modes,
            ..Default::default()
        };
        app.sync_display_modes();
        app.apply_poll_interval();

        (
            app,
            Task::perform(battery::poll_batteries(), |result| match result {
                Ok(data) => Message::Update(data),
                Err(e) => {
                    log::debug!("initial battery poll failed: {e}");
                    Message::NoBattery
                }
            })
            .map(Action::App),
        )
    }

    fn on_close_requested(&self, id: cosmic::iced::window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(popup_id) = self.popup.take() {
                    self.apply_poll_interval();
                    destroy_popup(popup_id)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);

                    self.apply_poll_interval();
                    get_popup(popup_settings)
                };
            }
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                    self.apply_poll_interval();
                }
            }
            Message::Update(data) => {
                log::debug!(
                    "battery update: {:.1}% {} {:.3}W",
                    data.percentage,
                    data.status,
                    data.energy_rate
                );
                self.watts = data.energy_rate;
                self.display_watts.go_mut(data.energy_rate as f32, Instant::now());
                self.percentage = data.percentage;
                self.status = data.status;
                self.time_to_empty = data.time_to_empty;
                self.time_to_full = data.time_to_full;
                self.energy = data.energy;
                self.energy_full = data.energy_full;
                self.no_battery = false;
                self.batteries = data.batteries;
            }
            Message::NoBattery => {
                if !self.no_battery {
                    log::warn!("No battery detected");
                }
                self.no_battery = true;
                self.watts = 0.0;
                self.percentage = 0.0;
                self.status = String::new();
                self.batteries = Vec::new();
            }
            Message::DisplayModeSelected(entity) => {
                let Some(mode) = self.display_modes.data::<PanelDisplay>(entity).copied() else {
                    return Task::none();
                };
                self.display_modes.activate(entity);
                self.config.panel_display = mode;
                self.apply_poll_interval();

                if let Some(handler) = &self.config_handler {
                    if let Err(e) = self.config.set_panel_display(handler, mode) {
                        log::warn!("failed to persist panel_display: {e}");
                    }
                }
            }
            Message::ConfigUpdated(config) => {
                self.config = config;
                self.sync_display_modes();
                self.apply_poll_interval();
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let animated = self.display_watts.interpolate_with(|v| v, Instant::now()) as f64;
        let content = text::body(self.format_panel_string(animated));

        // AppletIcon draws no background at rest, just a hover highlight, and takes
        // its text colour from the panel, so the applet sits among the other panel
        // icons instead of on a grey pill.
        let btn = button::custom(content)
            .class(cosmic::theme::Button::AppletIcon)
            .on_press_down(Message::TogglePopup)
            .padding([4, 8]);

        self.core.applet.autosize_window(btn).into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content: Vec<Element<Message>> = Vec::new();

        content.push(
            container(
                segmented_control::horizontal(&self.display_modes)
                    .on_activate(Message::DisplayModeSelected),
            )
            .padding([12, 12, 8, 12])
            .into(),
        );
        content.push(divider::horizontal::default().into());

        if self.no_battery {
            content.push(
                container(text::body("No battery detected")).padding(12).into(),
            );
            return self.core.applet.popup_container(column::with_children(content)).into();
        }

        let battery_icon = canvas::Canvas::<BatteryIcon, Message, Theme>::new(BatteryIcon {
            percentage: (self.percentage / 100.0) as f32,
            charging: self.status == "Charging",
        })
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(32.0));

        content.push(
            container(
                row![
                    battery_icon,
                    column![
                        text::title1(format!("{:.0}%", self.percentage)),
                        text::caption(&self.status),
                    ]
                    .spacing(0),
                ]
                .spacing(12)
                .align_y(cosmic::iced::core::Alignment::Center),
            )
            .padding([12, 12, 4, 12])
            .into(),
        );

        content.push(
            container(
                canvas::Canvas::<BatteryBar, Message, Theme>::new(BatteryBar {
                    percentage: (self.percentage / 100.0) as f32,
                })
                .width(Length::Fill)
                .height(Length::Fixed(4.0)),
            )
            .padding([4, 12, 12, 12])
            .into(),
        );

        if self.batteries.len() > 1 {
            content.push(divider::horizontal::default().into());
            for b in &self.batteries {
                let watts_str = if b.energy_rate > 0.0 {
                    let sign = if b.status == "Charging" { "+" } else { "-" };
                    format!("  {}{}", sign, Self::format_watts(b.energy_rate))
                } else {
                    String::new()
                };
                content.push(
                    container(
                        text::body(format!("{}  {:.0}%  {}{}", b.name, b.percentage, b.status, watts_str)),
                    )
                    .padding([4, 12])
                    .into(),
                );
            }
        }

        content.push(divider::horizontal::default().into());

        if self.watts > 0.0 {
            let label = if self.status == "Charging" {
                "Charge rate"
            } else {
                "Discharge rate"
            };
            content.push(
                container(
                    row![
                        text::body(label).width(cosmic::iced::Length::Fill),
                        text::body(Self::format_watts(self.watts)),
                    ]
                    .align_y(cosmic::iced::core::Alignment::Center),
                )
                .padding([6, 12])
                .into(),
            );
        }

        content.push(
            container(
                row![
                    text::body("Energy remaining").width(cosmic::iced::Length::Fill),
                    text::body(format!("{:.1} Wh", self.energy)),
                ]
                .align_y(cosmic::iced::core::Alignment::Center),
            )
            .padding([6, 12])
            .into(),
        );

        content.push(
            container(
                row![
                    text::body("Full capacity").width(cosmic::iced::Length::Fill),
                    text::body(format!("{:.1} Wh", self.energy_full)),
                ]
                .align_y(cosmic::iced::core::Alignment::Center),
            )
            .padding([6, 12])
            .into(),
        );

        if self.status == "Discharging" && self.time_to_empty > 0 {
            content.push(
                container(
                    row![
                        text::body("Time to empty").width(cosmic::iced::Length::Fill),
                        text::body(Self::format_time(self.time_to_empty)),
                    ]
                    .align_y(cosmic::iced::core::Alignment::Center),
                )
                .padding([6, 12])
                .into(),
            );
        }

        if self.status == "Charging" && self.time_to_full > 0 {
            content.push(
                container(
                    row![
                        text::body("Time to full").width(cosmic::iced::Length::Fill),
                        text::body(Self::format_time(self.time_to_full)),
                    ]
                    .align_y(cosmic::iced::core::Alignment::Center),
                )
                .padding([6, 12])
                .into(),
            );
        }

        self.core.applet.popup_container(column::with_children(content)).into()
    }

    fn subscription(&self) -> Subscription<Message> {
        let battery = Subscription::run_with(
            std::any::TypeId::of::<()>(),
            |_state| {
                futures_util::stream::unfold(
                    (),
                    |_| async move {
                        let message = match battery::poll_batteries().await {
                            Ok(data) => Some((Message::Update(data), ())),
                            Err(e) => {
                                log::debug!("poll_battery failed: {e}");
                                Some((Message::NoBattery, ()))
                            }
                        };
                        tokio::time::sleep(Duration::from_millis(POLL_MS.load(Ordering::Relaxed)))
                            .await;
                        message
                    },
                )
            },
        );

        // Picks up edits made to the config file directly, and keeps multiple
        // instances of the applet in agreement.
        let config = self
            .core
            .watch_config::<PowerMonitorConfig>(ID)
            .map(|update| {
                for err in update.errors {
                    log::warn!("config watch error: {err}");
                }
                Message::ConfigUpdated(update.config)
            });

        Subscription::batch(vec![battery, config])
    }
}

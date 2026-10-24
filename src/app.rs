use cosmic::app::Core;
use cosmic::cosmic_config::{Config, CosmicConfigEntry};
use cosmic::iced::core::animation::{Animation, Easing};
use cosmic::iced::mouse;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Length, Limits, Subscription};
use cosmic::widget::{
    button, canvas, column, container, divider, row, segmented_button, segmented_control, text,
    toggler,
};
use cosmic::{Action, Element, Task, Theme};
use cosmic::iced::Color;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::battery;
use crate::config::{PanelDisplay, PowerMonitorConfig};
use crate::fl;

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

/// How much battery-power history the popup trend covers.
const HISTORY_WINDOW: Duration = Duration::from_secs(30 * 60);
/// A chart needs more than two samples right after launch; hold the message
/// until there is at least half a minute of real data.
const TREND_MIN_SPAN: Duration = Duration::from_secs(30);
const TREND_BUCKETS: usize = 48;

/// A bounded ring of battery-power samples. The popup renders this, never a
/// fabricated waveform: empty buckets stay empty and the card shows a
/// collecting message until there is real data.
#[derive(Default)]
struct PowerHistory {
    samples: VecDeque<(Instant, f32)>,
}

impl PowerHistory {
    fn push(&mut self, at: Instant, watts: f64) {
        let cutoff = at.checked_sub(HISTORY_WINDOW).unwrap_or(at);

        let value = if watts.is_finite() {
            watts.abs() as f32
        } else {
            0.0
        };
        self.samples.push_back((at, value));

        while self
            .samples
            .front()
            .is_some_and(|(sample_at, _)| *sample_at < cutoff)
        {
            self.samples.pop_front();
        }
    }

    fn clear(&mut self) {
        self.samples.clear();
    }

    fn span(&self, now: Instant) -> Duration {
        let Some((first, _)) = self.samples.front() else {
            return Duration::ZERO;
        };
        now.saturating_duration_since(*first).min(HISTORY_WINDOW)
    }

    /// Average and peak from the raw samples, not from the downsampled chart
    /// buckets: a short spike must not disappear behind a bucket average.
    fn stats(&self, now: Instant) -> Option<(f32, f32)> {
        let (first, _) = self.samples.front()?;
        let cutoff = now.checked_sub(HISTORY_WINDOW).unwrap_or(now);
        let start = (*first).max(cutoff);

        let mut sum = 0.0_f32;
        let mut count = 0_u32;
        let mut peak = 0.0_f32;
        for (at, value) in &self.samples {
            if *at < start || *at > now {
                continue;
            }
            sum += *value;
            count += 1;
            peak = peak.max(*value);
        }

        (count > 0).then(|| (sum / count as f32, peak))
    }

    fn points(&self, now: Instant, buckets: usize) -> Vec<Option<f32>> {
        let Some((first, _)) = self.samples.front() else {
            return Vec::new();
        };
        if buckets == 0 || self.samples.len() < 2 {
            return Vec::new();
        }

        let cutoff = now.checked_sub(HISTORY_WINDOW).unwrap_or(now);
        let start = (*first).max(cutoff);
        let span = now.saturating_duration_since(start);
        if span < TREND_MIN_SPAN {
            return Vec::new();
        }

        let bucket_span = span.as_secs_f32() / buckets as f32;
        if bucket_span <= 0.0 {
            return Vec::new();
        }

        let mut sums = vec![0.0_f32; buckets];
        let mut counts = vec![0_u32; buckets];
        for (at, value) in &self.samples {
            if *at < start || *at > now {
                continue;
            }
            let offset = (*at).saturating_duration_since(start).as_secs_f32();
            let index = ((offset / bucket_span).floor() as usize).min(buckets - 1);
            sums[index] += *value;
            counts[index] += 1;
        }

        if !counts.iter().any(|count| *count > 0) {
            return Vec::new();
        }

        sums.into_iter()
            .zip(counts)
            .map(|(sum, count)| (count > 0).then(|| sum / count as f32))
            .collect()
    }
}

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
    energy_full_design: Option<f64>,
    cycle_count: Option<u64>,
    temperature: Option<f64>,
    no_battery: bool,
    config: PowerMonitorConfig,
    config_handler: Option<Config>,
    display_modes: segmented_button::SingleSelectModel,
    batteries: Vec<battery::BatteryInfo>,
    history: PowerHistory,
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
            energy_full_design: None,
            cycle_count: None,
            temperature: None,
            no_battery: false,
            config: PowerMonitorConfig::default(),
            config_handler: None,
            display_modes: segmented_button::SingleSelectModel::default(),
            batteries: Vec::new(),
            history: PowerHistory::default(),
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
    PanelIconToggled(bool),
    ConfigUpdated(PowerMonitorConfig),
}

impl PowerMonitor {
    /// Wattage at the precision the reading warrants: tenths above 1W,
    /// hundredths above 0.1W, thousandths below that, so an idle battery does
    /// not render a stream of zeros.
    fn format_watts_value(w: f64) -> String {
        if w >= 1.0 {
            format!("{:.1}", w)
        } else if w >= 0.1 {
            format!("{:.2}", w)
        } else {
            format!("{:.3}", w)
        }
    }

    fn format_watts(w: f64) -> String {
        format!("{}W", Self::format_watts_value(w))
    }

    /// Same value with a space before the unit, for the popup status line where
    /// the wattage reads on its own rather than inside a panel estimate.
    fn format_watts_labeled(w: f64) -> String {
        format!("{} W", Self::format_watts_value(w))
    }

    fn format_power_string(&self, watts: f64, with_time: bool) -> String {
        if self.no_battery {
            return String::new();
        }
        if matches!(self.status.as_str(), "Full" | "Fully Charged") {
            return fl!("panel-full").to_string();
        }
        if watts <= 0.0 {
            if self.status == "Charging" || self.status == "Not charging" {
                return fl!("panel-stopped").to_string();
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

    /// Capacity-based health, not laboratory-grade chemistry SoH: the kernel's
    /// own `full` over `full_design`. Treats a missing or zero design capacity
    /// as unknown rather than dividing by zero.
    fn health_percentage(&self) -> Option<f64> {
        let design = self.energy_full_design?;
        if design <= 0.0 || self.energy_full <= 0.0 {
            return None;
        }
        Some(self.energy_full / design * 100.0)
    }

    /// A label/value row with the label filling the width, so every value in the
    /// popup shares one right edge. The value carries more weight than its label
    /// at the same size, so measurements read first without competing with the
    /// percentage above them.
    fn detail_row<'a>(
        label: impl Into<std::borrow::Cow<'a, str>> + 'a,
        value: String,
    ) -> Element<'a, Message> {
        container(
            row![
                text::body(label).width(cosmic::iced::Length::Fill),
                text::body(value).font(cosmic::font::semibold()),
            ]
            .spacing(8)
            .align_y(cosmic::iced::core::Alignment::Center),
        )
        .padding([3, 12])
        .into()
    }

    /// Appends one rounded card holding a bold section title and its rows, with
    /// a hairline between each row. Callers skip the call entirely when a
    /// section has no rows, so no empty card is left behind.
    fn push_section<'a>(
        sections: &mut Vec<Element<'a, Message>>,
        label: String,
        rows: Vec<Element<'a, Message>>,
    ) {
        if rows.is_empty() {
            return;
        }

        let mut block: Vec<Element<'a, Message>> = Vec::with_capacity(rows.len() * 2);
        block.push(
            container(text::body(label).font(cosmic::font::semibold()))
                .padding([10, 12, 8, 12])
                .into(),
        );
        block.push(divider::horizontal::light().into());
        for (index, row) in rows.into_iter().enumerate() {
            if index > 0 {
                block.push(divider::horizontal::light().into());
            }
            block.push(row);
        }

        sections.push(
            container(column::with_children(block).spacing(0))
                .class(cosmic::theme::Container::Card)
                .into(),
        );
    }

    fn panel_footer(&self) -> Element<'_, Message> {
        let header: Element<Message> = if self.core.applet.is_horizontal() {
            row![
                text::caption(fl!("panel-display")),
                container(
                    toggler(self.config.show_panel_icon)
                        .label(fl!("panel-icon"))
                        .on_toggle(Message::PanelIconToggled)
                        .size(20.0)
                        .text_size(12.0)
                        .spacing(6.0),
                )
                .width(Length::Fill)
                .align_x(cosmic::iced::core::Alignment::End),
            ]
            .align_y(cosmic::iced::core::Alignment::Center)
            .into()
        } else {
            text::caption(fl!("panel-display")).into()
        };

        column![
            divider::horizontal::default(),
            container(
                column![
                    header,
                    segmented_control::horizontal(&self.display_modes)
                        .on_activate(Message::DisplayModeSelected),
                ]
                .spacing(6),
            )
            .padding([10, 12, 12, 12]),
        ]
        .into()
    }

    fn trend_card(
        &self,
        points: &[Option<f32>],
        span: Duration,
        now: Instant,
    ) -> Element<'_, Message> {
        let values: Vec<f32> = points.iter().flatten().copied().collect();
        let stats = self.history.stats(now);
        let window_label = if span >= HISTORY_WINDOW || span.is_zero() {
            fl!("trend-window")
        } else {
            Self::format_time(span.as_secs() as i64)
        };
        let summary = stats
            .map(|(avg, peak)| {
                format!(
                    "{} {avg:.1} W  {} {peak:.1} W",
                    fl!("trend-average"),
                    fl!("trend-peak")
                )
            })
            .unwrap_or_default();

        let mut card = column![
            row![
                text::body(fl!("trend-title")).font(cosmic::font::semibold()),
                container(text::caption(window_label))
                    .width(Length::Fill)
                    .align_x(cosmic::iced::core::Alignment::End),
            ]
            .align_y(cosmic::iced::core::Alignment::Center),
        ]
        .spacing(6);

        if values.len() >= 2 {
            card = card.push(
                canvas::Canvas::<TrendChart, Message, Theme>::new(TrendChart {
                    points: points.to_vec(),
                    charging: self.status == "Charging",
                })
                .width(Length::Fill)
                .height(Length::Fixed(56.0)),
            );
            card = card.push(
                row![
                    text::caption(format!(
                        "{} {}",
                        fl!("trend-now"),
                        Self::format_watts_labeled(self.watts.abs())
                    )),
                    container(text::caption(summary))
                        .width(Length::Fill)
                        .align_x(cosmic::iced::core::Alignment::End),
                ]
                .align_y(cosmic::iced::core::Alignment::Center),
            );
        } else {
            card = card.push(
                container(text::caption(fl!("trend-collecting")))
                    .width(Length::Fill)
                    .padding([18, 0]),
            );
        }

        container(card)
            .padding(12)
            .class(cosmic::theme::Container::Card)
            .into()
    }

    fn popup_body(&self) -> Vec<Element<'_, Message>> {
        let mut content: Vec<Element<Message>> = Vec::new();

        if self.no_battery {
            content.push(
                container(
                    column![
                        text::title3(fl!("no-battery")),
                        text::caption(fl!("no-battery-caption")),
                    ]
                    .spacing(4)
                    .align_x(cosmic::iced::core::Alignment::Center),
                )
                .padding([32, 12])
                .width(Length::Fill)
                .into(),
            );
            content.push(self.panel_footer());
            return content;
        }

        let battery_icon = canvas::Canvas::<BatteryIcon, Message, Theme>::new(BatteryIcon {
            percentage: (self.percentage / 100.0) as f32,
            charging: self.status == "Charging",
        })
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(32.0));

        let mut status_parts = vec![localize_status(&self.status)];
        if self.watts > 0.0 {
            let sign = match self.status.as_str() {
                "Charging" => "+",
                "Discharging" => "-",
                _ => "",
            };
            status_parts.push(format!(
                "{}{}",
                sign,
                Self::format_watts_labeled(self.watts.abs())
            ));
        }
        let time = if self.status == "Discharging" {
            self.time_to_empty
        } else if self.status == "Charging" {
            self.time_to_full
        } else {
            0
        };
        if time > 0 {
            status_parts.push(Self::format_time(time));
        }
        let status_line = status_parts.join(" \u{b7} ");

        let mut body: Vec<Element<Message>> = Vec::new();

        body.push(
            container(
                column![
                    row![
                        battery_icon,
                        column![
                            text::title1(format!("{:.0}%", self.percentage)),
                            text::caption(status_line),
                        ]
                        .spacing(0),
                    ]
                    .spacing(12)
                    .align_y(cosmic::iced::core::Alignment::Center),
                    canvas::Canvas::<BatteryBar, Message, Theme>::new(BatteryBar {
                        percentage: (self.percentage / 100.0) as f32,
                    })
                    .width(Length::Fill)
                    .height(Length::Fixed(4.0)),
                ]
                .spacing(10),
            )
            .padding(12)
            .class(cosmic::theme::Container::Card)
            .into(),
        );

        let now = Instant::now();
        let points = self.history.points(now, TREND_BUCKETS);
        let span = self.history.span(now);
        body.push(self.trend_card(&points, span, now));

        let mut sections: Vec<Element<Message>> = Vec::new();

        if self.batteries.len() > 1 {
            let mut battery_rows: Vec<Element<Message>> = Vec::new();
            for b in &self.batteries {
                let status = if b.energy_rate > 0.0 {
                    let sign = if b.status == "Charging" { "+" } else { "-" };
                    format!(
                        "{} \u{b7} {}{}",
                        localize_status(&b.status),
                        sign,
                        Self::format_watts_labeled(b.energy_rate)
                    )
                } else {
                    localize_status(&b.status)
                };
                battery_rows.push(
                    container(
                        column![
                            row![
                                text::body(&b.name).font(cosmic::font::semibold()),
                                container(text::caption(format!("{:.0}%", b.percentage)))
                                    .width(Length::Fill)
                                    .align_x(cosmic::iced::core::Alignment::End),
                            ]
                            .align_y(cosmic::iced::core::Alignment::Center),
                            canvas::Canvas::<MiniBar, Message, Theme>::new(MiniBar {
                                percentage: (b.percentage / 100.0) as f32,
                            })
                            .width(Length::Fill)
                            .height(Length::Fixed(5.0)),
                            text::caption(status),
                        ]
                        .spacing(5),
                    )
                    .padding([8, 12])
                    .into(),
                );
            }
            Self::push_section(&mut sections, fl!("batteries"), battery_rows);
        }

        let mut battery_rows: Vec<Element<Message>> = vec![
            Self::detail_row(fl!("energy-remaining"), format!("{:.1} Wh", self.energy)),
            Self::detail_row(fl!("full-capacity"), format!("{:.1} Wh", self.energy_full)),
        ];
        if let Some(design) = self.energy_full_design.filter(|design| *design > 0.0) {
            battery_rows.push(Self::detail_row(
                fl!("design-capacity"),
                format!("{:.1} Wh", design),
            ));
        }
        if self.status == "Discharging" && self.time_to_empty > 0 {
            battery_rows.push(Self::detail_row(
                fl!("time-to-empty"),
                Self::format_time(self.time_to_empty),
            ));
        }
        if self.status == "Charging" && self.time_to_full > 0 {
            battery_rows.push(Self::detail_row(
                fl!("time-to-full"),
                Self::format_time(self.time_to_full),
            ));
        }
        Self::push_section(&mut sections, fl!("section-battery"), battery_rows);

        let mut health_rows: Vec<Element<Message>> = Vec::new();
        if self.batteries.len() == 2 {
            // The card title already localizes "Health"; names in the row keep
            // this value on one line at 360px in every locale.
            let health: Vec<(&str, f64)> = self
                .batteries
                .iter()
                .filter_map(|b| {
                    b.health_percentage()
                        .map(|health| (b.name.as_str(), health))
                })
                .collect();
            if !health.is_empty() {
                let names = health
                    .iter()
                    .map(|(name, _)| *name)
                    .collect::<Vec<_>>()
                    .join("/");
                let values = health
                    .iter()
                    .map(|(_, health)| format!("{:.0}%", health))
                    .collect::<Vec<_>>()
                    .join("/");
                health_rows.push(Self::detail_row(names, values));
            }
        } else if self.batteries.len() > 2 {
            for b in &self.batteries {
                if let Some(health) = b.health_percentage() {
                    health_rows.push(Self::detail_row(
                        b.name.as_str(),
                        format!("{:.0}%", health),
                    ));
                }
            }
        } else if let Some(health) = self.health_percentage() {
            health_rows.push(Self::detail_row(
                fl!("battery-health"),
                format!("{:.0}%", health),
            ));
        }
        if let Some(temp) = self.temperature {
            health_rows.push(Self::detail_row(
                fl!("temperature"),
                Self::format_temperature(temp),
            ));
        }
        if let Some(cycles) = self.cycle_count {
            health_rows.push(Self::detail_row(fl!("cycle-count"), format!("{cycles}")));
        }
        if !health_rows.is_empty() {
            Self::push_section(&mut sections, fl!("section-health"), health_rows);
        }

        body.push(container(column::with_children(sections).spacing(8)).into());

        content.push(
            container(column::with_children(body).spacing(8))
                .padding([12, 12, 0, 12])
                .into(),
        );
        content.push(self.panel_footer());
        content
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
            format!("{}{} {}{}", hours, fl!("hours"), minutes, fl!("minutes"))
        } else if minutes > 0 {
            format!("{}{}", minutes, fl!("minutes"))
        } else {
            format!("{}{}", seconds, fl!("seconds"))
        }
    }

    fn format_temperature(temp: f64) -> String {
        format!("{temp:.1}{}", fl!("degrees"))
    }
}

fn poll_task() -> Task<Action<Message>> {
    Task::perform(battery::poll_batteries(), |result| match result {
        Ok(data) => Message::Update(data),
        Err(e) => {
            log::debug!("battery poll failed: {e}");
            Message::NoBattery
        }
    })
    .map(Action::App)
}

/// The kernel reports a fixed set of status strings. Name the ones we know how
/// to name and pass anything else through, so an unexpected status still shows.
fn localize_status(status: &str) -> String {
    match status {
        "Charging" => fl!("status-charging"),
        "Discharging" => fl!("status-discharging"),
        "Full" | "Fully Charged" => fl!("status-full"),
        "Not charging" => fl!("status-not-charging"),
        other => other.to_string(),
    }
}

fn battery_fill_color(theme: &Theme, pct: f32) -> Color {
    if pct > 0.20 {
        theme.cosmic().success_color().into()
    } else if pct > 0.10 {
        theme.cosmic().warning_color().into()
    } else {
        theme.cosmic().destructive_color().into()
    }
}

fn battery_on_color(theme: &Theme, pct: f32) -> Color {
    if pct > 0.20 {
        theme.cosmic().on_success_color().into()
    } else if pct > 0.10 {
        theme.cosmic().on_warning_color().into()
    } else {
        theme.cosmic().on_destructive_color().into()
    }
}

fn battery_track_color(theme: &Theme) -> Color {
    let divider: Color = theme.cosmic().background(theme.transparent).divider.into();
    Color::from_rgba(divider.r, divider.g, divider.b, 0.25)
}

fn battery_outline_color(theme: &Theme) -> Color {
    let divider: Color = theme.cosmic().background(theme.transparent).divider.into();
    Color::from_rgba(divider.r, divider.g, divider.b, 0.65)
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
        theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let width = bounds.width;
        let height = bounds.height;
        let radius = height / 2.0;
        let percentage = self.percentage.clamp(0.0, 1.0);
        let fill_width = width * percentage;

        let track = canvas::Path::new(|b| {
            b.rounded_rectangle(
                cosmic::iced::Point::new(0.0, 0.0),
                cosmic::iced::Size::new(width, height),
                cosmic::iced::border::Radius::from(radius),
            )
        });
        frame.fill(&track, battery_track_color(theme));

        if fill_width > 0.0 {
            let fill = canvas::Path::new(|b| {
                b.rounded_rectangle(
                    cosmic::iced::Point::new(0.0, 0.0),
                    cosmic::iced::Size::new(fill_width, height),
                    cosmic::iced::border::Radius::from(radius.min(fill_width / 2.0)),
                )
            });
            frame.fill(&fill, battery_fill_color(theme, percentage));
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
        theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let body = cosmic::iced::Rectangle::new(
            cosmic::iced::Point::new(1.0, 1.0),
            cosmic::iced::Size::new(44.0, 30.0),
        );
        let fill_color = battery_fill_color(theme, self.percentage.clamp(0.0, 1.0));
        let outline = battery_outline_color(theme);

        let body_path = canvas::Path::new(|b| {
            b.rounded_rectangle(
                body.position(),
                body.size(),
                cosmic::iced::border::Radius::from(4.0),
            )
        });

        // Fill proportionally so the icon reads as an actual level rather than
        // only a colour band. The colour bands stay, so it still flags a low
        // battery without having to compare it against the bar underneath.
        let fill_width = (body.width - 8.0) * self.percentage.clamp(0.0, 1.0);
        if fill_width > 0.0 {
            // Clamp the corner radius to half the fill width: at a few percent
            // charge the fill is under a pixel wide, and a fixed radius there is
            // degenerate geometry that can render as a blob or drop out.
            let fill_path = canvas::Path::new(|b| {
                b.rounded_rectangle(
                    cosmic::iced::Point::new(4.0, 4.0),
                    cosmic::iced::Size::new(fill_width, body.height - 8.0),
                    cosmic::iced::border::Radius::from(2.0_f32.min(fill_width / 2.0)),
                )
            });
            frame.fill(&fill_path, fill_color);
        }
        frame.stroke(
            &body_path,
            canvas::Stroke::default().with_width(2.0).with_color(outline),
        );

        // Terminal, in the shell colour so it reads as part of the outline.
        let nub = canvas::Path::new(|b| {
            b.rounded_rectangle(
                cosmic::iced::Point::new(45.0, 11.0),
                cosmic::iced::Size::new(3.0, 10.0),
                cosmic::iced::border::Radius::from(1.0),
            )
        });
        frame.fill(&nub, outline);

        if self.charging && self.percentage < 1.0 {
            let bolt = canvas::Path::new(|b| {
                b.move_to(cosmic::iced::Point::new(21.0, 6.0));
                b.line_to(cosmic::iced::Point::new(16.0, 17.4));
                b.line_to(cosmic::iced::Point::new(20.5, 17.4));
                b.line_to(cosmic::iced::Point::new(20.0, 25.0));
                b.line_to(cosmic::iced::Point::new(25.0, 13.6));
                b.line_to(cosmic::iced::Point::new(20.5, 13.6));
                b.close();
            });
            let bolt_color = battery_on_color(theme, self.percentage.clamp(0.0, 1.0));
            frame.fill(&bolt, bolt_color);
            frame.stroke(
                &bolt,
                canvas::Stroke::default()
                    .with_width(2.0)
                    .with_color(bolt_color)
                    .with_line_join(canvas::LineJoin::Round),
            );
        }

        vec![frame.into_geometry()]
    }
}

struct MiniBar {
    percentage: f32,
}

impl canvas::Program<Message, cosmic::Theme> for MiniBar {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::iced::Renderer,
        theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let width = bounds.width;
        let height = bounds.height;
        let radius = height / 2.0;
        let percentage = self.percentage.clamp(0.0, 1.0);

        let track = canvas::Path::new(|b| {
            b.rounded_rectangle(
                cosmic::iced::Point::new(0.0, 0.0),
                cosmic::iced::Size::new(width, height),
                cosmic::iced::border::Radius::from(radius),
            )
        });
        frame.fill(&track, battery_track_color(theme));

        let fill_width = width * percentage;
        if fill_width > 0.0 {
            let fill = canvas::Path::new(|b| {
                b.rounded_rectangle(
                    cosmic::iced::Point::new(0.0, 0.0),
                    cosmic::iced::Size::new(fill_width, height),
                    cosmic::iced::border::Radius::from(radius.min(fill_width / 2.0)),
                )
            });
            frame.fill(&fill, battery_fill_color(theme, percentage));
        }

        vec![frame.into_geometry()]
    }
}

struct TrendChart {
    points: Vec<Option<f32>>,
    charging: bool,
}

impl canvas::Program<Message, cosmic::Theme> for TrendChart {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::iced::Renderer,
        theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let available: Vec<(usize, f32)> = self
            .points
            .iter()
            .enumerate()
            .filter_map(|(index, value)| value.map(|value| (index, value)))
            .collect();
        if available.len() < 2 {
            return vec![frame.into_geometry()];
        }

        let width = bounds.width;
        let height = bounds.height;
        let pad = 2.0;
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for (_, value) in &available {
            min = min.min(*value);
            max = max.max(*value);
        }
        let min = (min - 1.0).max(0.0);
        let max = max + 1.0;
        let span = (max - min).max(1.0);

        let last_index = (self.points.len() - 1) as f32;
        let x_at = |index: usize| pad + (index as f32 / last_index) * (width - pad * 2.0);
        let y_at = |value: f32| height - pad - ((value - min) / span) * (height - pad * 2.0);

        let accent: Color = if self.charging {
            theme.cosmic().success_color().into()
        } else {
            theme.cosmic().accent_color().into()
        };
        let area_color = Color::from_rgba(accent.r, accent.g, accent.b, 0.18);

        let area = canvas::Path::new(|b| {
            b.move_to(cosmic::iced::Point::new(
                x_at(available[0].0),
                height - pad,
            ));
            for (index, value) in &available {
                b.line_to(cosmic::iced::Point::new(x_at(*index), y_at(*value)));
            }
            b.line_to(cosmic::iced::Point::new(
                x_at(available[available.len() - 1].0),
                height - pad,
            ));
            b.close();
        });
        frame.fill(&area, area_color);

        let line = canvas::Path::new(|b| {
            b.move_to(cosmic::iced::Point::new(
                x_at(available[0].0),
                y_at(available[0].1),
            ));
            for (index, value) in available.iter().skip(1) {
                b.line_to(cosmic::iced::Point::new(x_at(*index), y_at(*value)));
            }
        });
        frame.stroke(
            &line,
            canvas::Stroke::default().with_width(2.0).with_color(accent),
        );

        vec![frame.into_geometry()]
    }
}

struct PanelBatteryIcon {
    percentage: f32,
    charging: bool,
}

impl canvas::Program<Message, cosmic::Theme> for PanelBatteryIcon {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &cosmic::iced::Renderer,
        theme: &Theme,
        bounds: cosmic::iced::Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let width = bounds.width;
        let height = bounds.height;
        let percentage = self.percentage.clamp(0.0, 1.0);
        let fill_color = battery_fill_color(theme, percentage);
        let outline = battery_outline_color(theme);

        let body = cosmic::iced::Rectangle::new(
            cosmic::iced::Point::new(0.5, 0.5),
            cosmic::iced::Size::new(width - 5.0, height - 1.0),
        );
        let body_path = canvas::Path::new(|b| {
            b.rounded_rectangle(
                body.position(),
                body.size(),
                cosmic::iced::border::Radius::from(2.0),
            )
        });

        let fill_width = (body.width - 3.0) * percentage;
        if fill_width > 0.0 {
            let fill = canvas::Path::new(|b| {
                b.rounded_rectangle(
                    cosmic::iced::Point::new(2.0, 2.0),
                    cosmic::iced::Size::new(fill_width, body.height - 3.0),
                    cosmic::iced::border::Radius::from(1.0_f32.min(fill_width / 2.0)),
                )
            });
            frame.fill(&fill, fill_color);
        }
        frame.stroke(
            &body_path,
            canvas::Stroke::default().with_width(1.0).with_color(outline),
        );

        let nub = canvas::Path::new(|b| {
            b.rounded_rectangle(
                cosmic::iced::Point::new(width - 4.0, height / 2.0 - 2.0),
                cosmic::iced::Size::new(2.5, 4.0),
                cosmic::iced::border::Radius::from(1.0),
            )
        });
        frame.fill(&nub, outline);

        if self.charging {
            let bolt = canvas::Path::new(|b| {
                b.move_to(cosmic::iced::Point::new(10.0, 3.0));
                b.line_to(cosmic::iced::Point::new(7.5, 8.0));
                b.line_to(cosmic::iced::Point::new(10.0, 8.0));
                b.line_to(cosmic::iced::Point::new(9.5, 11.5));
                b.line_to(cosmic::iced::Point::new(12.5, 7.0));
                b.line_to(cosmic::iced::Point::new(10.0, 7.0));
                b.close();
            });
            frame.fill(&bolt, battery_on_color(theme, percentage));
        }

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

        (app, poll_task())
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
                    Task::batch([get_popup(popup_settings), poll_task()])
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
                let now = Instant::now();
                self.display_watts.go_mut(data.energy_rate as f32, now);
                self.percentage = data.percentage;
                self.status = data.status;
                self.time_to_empty = data.time_to_empty;
                self.time_to_full = data.time_to_full;
                self.energy = data.energy;
                self.energy_full = data.energy_full;
                self.energy_full_design = data.energy_full_design;
                self.cycle_count = data.cycle_count;
                self.temperature = data.temperature;
                self.no_battery = false;
                self.history.push(now, data.energy_rate);
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
                self.energy_full_design = None;
                self.cycle_count = None;
                self.temperature = None;
                self.history.clear();
                self.batteries = Vec::new();
            }
            Message::DisplayModeSelected(entity) => {
                let Some(mode) = self.display_modes.data::<PanelDisplay>(entity).copied() else {
                    return Task::none();
                };
                self.display_modes.activate(entity);

                if let Some(handler) = &self.config_handler {
                    if let Err(e) = self.config.set_panel_display(handler, mode) {
                        log::warn!("failed to persist panel_display: {e}");
                    }
                } else {
                    self.config.panel_display = mode;
                }

                self.apply_poll_interval();
            }
            Message::PanelIconToggled(enabled) => {
                if let Some(handler) = &self.config_handler {
                    if let Err(e) = self.config.set_show_panel_icon(handler, enabled) {
                        log::warn!("failed to persist show_panel_icon: {e}");
                    }
                } else {
                    self.config.show_panel_icon = enabled;
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
        let label = text::body(self.format_panel_string(animated));

        let content: Element<Message> = if self.core.applet.is_horizontal()
            && !self.no_battery
            && self.config.show_panel_icon
        {
            let glyph = canvas::Canvas::<PanelBatteryIcon, Message, Theme>::new(PanelBatteryIcon {
                percentage: (self.percentage / 100.0) as f32,
                charging: self.status == "Charging",
            })
            .width(Length::Fixed(22.0))
            .height(Length::Fixed(14.0));

            row![glyph, label]
                .spacing(6)
                .align_y(cosmic::iced::core::Alignment::Center)
                .into()
        } else {
            label.into()
        };

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
        self.core
            .applet
            .popup_container(column::with_children(self.popup_body()))
            .into()
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

#[cfg(test)]
mod tests {
    use super::{PowerHistory, PowerMonitor, HISTORY_WINDOW};
    use std::time::{Duration, Instant};

    /// Builds the health inputs directly so the ratio can be checked against
    /// the values real hardware reports.
    fn health_for(energy_full: f64, design: Option<f64>) -> Option<f64> {
        let design = design?;
        if design <= 0.0 || energy_full <= 0.0 {
            return None;
        }
        Some(energy_full / design * 100.0)
    }

    #[test]
    fn health_is_one_hundred_when_full_matches_design() {
        // BAT0 on this machine: energy_full == energy_full_design == 49.815 Wh
        assert_eq!(health_for(49.815, Some(49.815)), Some(100.0));
    }

    #[test]
    fn health_scales_down_with_capacity_loss() {
        let h = health_for(39.83, Some(49.815)).unwrap();
        assert!((h - 79.955).abs() < 0.01, "got {h}");
    }

    #[test]
    fn health_is_none_without_design_capacity() {
        assert_eq!(health_for(49.815, None), None);
    }

    #[test]
    fn health_is_none_on_zero_design_capacity() {
        assert_eq!(health_for(49.815, Some(0.0)), None);
        assert_eq!(health_for(49.815, Some(-1.0)), None);
    }

    #[test]
    fn health_is_none_on_zero_full_capacity() {
        assert_eq!(health_for(0.0, Some(49.815)), None);
    }

    #[test]
    fn temperature_keeps_tenths() {
        let formatted = PowerMonitor::format_temperature(30.5);
        assert!(formatted.starts_with("30.5"), "got {formatted}");
        assert!(formatted.ends_with("°C"), "got {formatted}");
    }

    #[test]
    fn history_discards_samples_older_than_window() {
        let now = Instant::now();
        let mut history = PowerHistory::default();
        history.push(now - HISTORY_WINDOW - Duration::from_secs(1), 5.0);
        history.push(now, 7.0);

        assert_eq!(history.span(now), Duration::ZERO);
        assert!(history.points(now, 4).is_empty());
    }

    #[test]
    fn history_waits_for_minimum_span() {
        let now = Instant::now();
        let mut history = PowerHistory::default();
        history.push(now - Duration::from_secs(10), 5.0);
        history.push(now, 7.0);

        assert!(history.points(now, 4).is_empty());
    }

    #[test]
    fn history_resamples_real_samples_and_keeps_empty_buckets_empty() {
        let now = Instant::now();
        let mut history = PowerHistory::default();
        history.push(now - Duration::from_secs(120), 10.0);
        history.push(now - Duration::from_secs(60), 20.0);
        history.push(now, 30.0);

        let points = history.points(now, 4);
        assert_eq!(points.len(), 4);
        assert_eq!(points.iter().flatten().count(), 3);
        assert!(points.iter().any(Option::is_none));
        assert_eq!(points[0], Some(10.0));
        assert_eq!(points[2], Some(20.0));
        assert_eq!(points[3], Some(30.0));
        assert_eq!(history.span(now), Duration::from_secs(120));
    }

    #[test]
    fn history_stats_use_raw_samples_for_peak() {
        let now = Instant::now();
        let mut history = PowerHistory::default();
        history.push(now - Duration::from_secs(300), 10.0);
        history.push(now - Duration::from_secs(290), 40.0);
        history.push(now - Duration::from_secs(200), 20.0);
        history.push(now - Duration::from_secs(100), 5.0);

        let points = history.points(now, 4);
        assert_eq!(points[0], Some(25.0));
        assert_eq!(points[1], Some(20.0));
        assert_eq!(points[2], Some(5.0));

        let (avg, peak) = history.stats(now).unwrap();
        assert!((avg - 18.75).abs() < 0.0001, "got {avg}");
        assert_eq!(peak, 40.0);
    }
}

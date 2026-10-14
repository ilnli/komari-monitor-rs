use crate::command_parser::{NetworkConfig, NetworkStatisticsMode, TrafficPeriod};
use crate::get_info::network::{filter_network, update_traffic_offset};
use log::{error, info, warn};
use std::fs;
use std::time::Duration;
use sysinfo::Networks;
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime, UtcOffset, Weekday};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

/// Represents the state of network statistics saved to disk
#[derive(PartialEq, Clone, Debug)]
struct NetworkInfo {
    config: NetworkConfig,
    boot_id: String,
    /// The total transmitted bytes accumulated in the current cycle.
    cycle_total_tx: u64,
    /// The total received bytes accumulated in the current cycle.
    cycle_total_rx: u64,
    /// Unix timestamp for the next scheduled reset.
    next_reset_timestamp: i64,
    offset_tx: i64,
    offset_rx: i64,
}

impl NetworkInfo {
    fn update_totals(&mut self, current_tx: u64, current_rx: u64) {
        // Kernel counters can restart when an interface is recreated, even if
        // the host did not reboot. Rebase without losing the saved cycle or
        // counting unaffected interfaces' lifetime traffic a second time.
        if (current_tx as i64) < self.cycle_total_tx as i64 - self.offset_tx {
            self.offset_tx = self.cycle_total_tx as i64 - current_tx as i64;
        }
        if (current_rx as i64) < self.cycle_total_rx as i64 - self.offset_rx {
            self.offset_rx = self.cycle_total_rx as i64 - current_rx as i64;
        }
        self.cycle_total_tx = (current_tx as i64 + self.offset_tx).max(0) as u64;
        self.cycle_total_rx = (current_rx as i64 + self.offset_rx).max(0) as u64;
    }

    pub fn encode(&self) -> String {
        let mut output = String::new();
        // Helper macro
        macro_rules! append_line {
            ($key:expr, $value:expr) => {
                output.push_str(&format!("{}={}\n", $key, $value));
            };
        }

        // Config fields
        append_line!(
            "disable_network_statistics",
            self.config.disable_network_statistics
        );
        append_line!("network_interval", self.config.network_interval);
        append_line!("network_save_path", &self.config.network_save_path);
        append_line!(
            "traffic_period",
            format!("{:?}", self.config.traffic_period)
        );
        append_line!("traffic_reset_day", &self.config.traffic_reset_day);
        append_line!(
            "network_statistics_mode",
            format!("{:?}", self.config.network_statistics_mode)
        );
        append_line!("network_duration", self.config.network_duration);
        append_line!(
            "network_interval_number",
            self.config.network_interval_number
        );

        // NetworkInfo fields
        append_line!("boot_id", &self.boot_id);
        append_line!("cycle_total_tx", self.cycle_total_tx);
        append_line!("cycle_total_rx", self.cycle_total_rx);
        append_line!("next_reset_timestamp", self.next_reset_timestamp);
        append_line!("offset_tx", self.offset_tx);
        append_line!("offset_rx", self.offset_rx);

        output
    }

    pub fn decode(input: &str) -> Result<Self, String> {
        // For NetworkConfig
        let mut disable_network_statistics = None;
        let mut network_interval = None;
        let mut network_save_path = None;
        let mut traffic_period = None;
        let mut traffic_reset_day = None;
        let mut network_statistics_mode = None;
        let mut network_duration = None;
        let mut network_interval_number = None;

        // For NetworkInfo
        let mut boot_id = None;
        let mut cycle_total_tx = None;
        let mut cycle_total_rx = None;
        let mut next_reset_timestamp = None;
        // Default to sentinel value if not found
        let mut offset_tx = i64::MIN;
        let mut offset_rx = i64::MIN;

        for line in input.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("Format error: expected key=value, got '{}'", line))?;
            let key = key.trim();
            let value = value.trim();

            let parse_err = |type_name: &str| format!("Invalid {} for key '{}'", type_name, key);

            match key {
                // Config
                "disable_network_statistics" => {
                    disable_network_statistics =
                        Some(value.parse::<bool>().map_err(|_| parse_err("bool"))?)
                }
                "network_interval" => {
                    network_interval = Some(value.parse::<u32>().map_err(|_| parse_err("u32"))?)
                }
                "network_save_path" => network_save_path = Some(value.to_string()),
                "traffic_period" => {
                    traffic_period = match value {
                        "Week" => Some(TrafficPeriod::Week),
                        "Month" => Some(TrafficPeriod::Month),
                        "Year" => Some(TrafficPeriod::Year),
                        _ => None,
                    };
                }
                "traffic_reset_day" => traffic_reset_day = Some(value.to_string()),
                "network_statistics_mode" => {
                    network_statistics_mode = match value {
                        "Natural" => Some(NetworkStatisticsMode::Natural),
                        "Fixed" => Some(NetworkStatisticsMode::Fixed),
                        _ => None,
                    };
                }
                "network_duration" => {
                    network_duration = Some(value.parse::<u32>().map_err(|_| parse_err("u32"))?)
                }
                "network_interval_number" => {
                    network_interval_number =
                        Some(value.parse::<u32>().map_err(|_| parse_err("u32"))?)
                }

                // Info
                "boot_id" => boot_id = Some(value.to_string()),
                "cycle_total_tx" => {
                    cycle_total_tx = Some(value.parse::<u64>().map_err(|_| parse_err("u64"))?)
                }
                "cycle_total_rx" => {
                    cycle_total_rx = Some(value.parse::<u64>().map_err(|_| parse_err("u64"))?)
                }
                "next_reset_timestamp" => {
                    next_reset_timestamp = Some(value.parse::<i64>().map_err(|_| parse_err("i64"))?)
                }
                "offset_tx" => offset_tx = value.parse::<i64>().map_err(|_| parse_err("i64"))?,
                "offset_rx" => offset_rx = value.parse::<i64>().map_err(|_| parse_err("i64"))?,
                _ => {} // Ignore unknown keys
            }
        }

        // Assemble the struct
        Ok(NetworkInfo {
            config: NetworkConfig {
                disable_network_statistics: disable_network_statistics
                    .ok_or("Missing field: disable_network_statistics")?,
                network_interval: network_interval.ok_or("Missing field: network_interval")?,
                network_save_path: network_save_path.ok_or("Missing field: network_save_path")?,
                traffic_period: traffic_period.unwrap_or(TrafficPeriod::Month),
                traffic_reset_day: traffic_reset_day.unwrap_or_else(|| "1".to_string()),
                network_statistics_mode: network_statistics_mode
                    .unwrap_or(NetworkStatisticsMode::Fixed),
                network_duration: network_duration.unwrap_or(864_000),
                network_interval_number: network_interval_number.unwrap_or(6),
            },
            boot_id: boot_id.ok_or("Missing field: boot_id")?,
            cycle_total_tx: cycle_total_tx.ok_or("Missing field: cycle_total_tx")?,
            cycle_total_rx: cycle_total_rx.ok_or("Missing field: cycle_total_rx")?,
            next_reset_timestamp: next_reset_timestamp
                .ok_or("Missing field: next_reset_timestamp")?,
            offset_tx,
            offset_rx,
        })
    }
}

/// Main entry point for the network statistics persistence thread.
pub async fn network_saver(
    network_config: &NetworkConfig,
    ready: tokio::sync::oneshot::Sender<()>,
) {
    if network_config.disable_network_statistics {
        return;
    }

    let mut ready = Some(ready);
    let mut networks = Networks::new_with_refreshed_list();

    loop {
        // Initialize state, handles file creation, migration, and reset logic
        let (mut file, mut network_info) = match initialize_network_state_and_offset(
            network_config,
            &mut networks,
        )
        .await
        {
            Ok(state) => state,
            Err(e) => {
                error!(
                    "Failed to initialize network statistics: {}. This feature will be disabled.",
                    e
                );
                return;
            }
        };

        if let Some(ready) = ready.take() {
            let _ = ready.send(());
        }
        info!(
            "Network statistics cycle started. Next reset on: {}",
            server_local_time(
                OffsetDateTime::from_unix_timestamp(network_info.next_reset_timestamp).unwrap()
            )
            .map(|reset| reset.format(&Rfc3339).unwrap())
            .unwrap_or_else(|e| format!("Unix {} ({e})", network_info.next_reset_timestamp))
        );

        // Add a counter to accumulate memory update times
        let mut memory_update_count = 0;

        // Main loop for the current cycle
        loop {
            // Wake at the boundary even when the persistence sampling interval
            // is longer, so the previous cycle is not reported after midnight.
            let until_reset = (network_info.next_reset_timestamp
                - OffsetDateTime::now_utc().unix_timestamp())
            .max(0) as u64;
            tokio::time::sleep(Duration::from_secs(
                u64::from(network_config.network_interval).min(until_reset),
            ))
            .await;

            let now = OffsetDateTime::now_utc().unix_timestamp();
            if now >= network_info.next_reset_timestamp {
                info!("Network statistics cycle ended. Resetting...");
                break; // Break inner loop to re-initialize
            }

            networks.refresh(true);
            let (_, _, current_total_tx, current_total_rx) = filter_network(&networks);

            network_info.update_totals(current_total_tx, current_total_rx);
            update_traffic_offset(network_info.offset_tx, network_info.offset_rx);

            memory_update_count += 1;
            if memory_update_count >= network_config.network_interval_number {
                // Save the updated state to the file
                if let Err(e) = save_network_info(&mut file, &network_info).await {
                    error!("Failed to save network statistics file: {}", e);
                    // Continue, maybe it's a temporary issue
                } else {
                    info!("Network statistics saved.");
                }
                memory_update_count = 0;
            }
        }
    }
}

/// Handles all startup logic: reading/creating the state file, migrating old formats,
/// handling reboots vs. restarts, and calculating the initial traffic offset.
async fn initialize_network_state_and_offset(
    network_config: &NetworkConfig,
    networks: &mut Networks,
) -> Result<(File, NetworkInfo), String> {
    // Ensure parent directory exists
    let save_path = std::path::Path::new(&network_config.network_save_path);
    if let Some(parent) = save_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create parent directory: {e}"))?;
            info!("Created parent directory: {}", parent.display());
        }
    }

    let mut file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&network_config.network_save_path)
        .await
    {
        Ok(f) => f,
        Err(e) => return Err(format!("Failed to open network state file: {e}")),
    };

    let mut raw_data = String::new();
    file.read_to_string(&mut raw_data)
        .await
        .map_err(|e| format!("Failed to read network state file: {e}"))?;

    networks.refresh(true);
    let (_, _, current_total_tx, current_total_rx) = filter_network(networks);
    let network_info = initialize_network_info(
        network_config,
        &raw_data,
        OffsetDateTime::now_utc(),
        get_boot_id(),
        current_total_tx,
        current_total_rx,
    )?;

    // Publish the offset only after initialization has been saved successfully.
    save_network_info(&mut file, &network_info)
        .await
        .map_err(|e| format!("Failed to save initial network state: {e}"))?;
    update_traffic_offset(network_info.offset_tx, network_info.offset_rx);

    Ok((file, network_info))
}

fn initialize_network_info(
    network_config: &NetworkConfig,
    raw_data: &str,
    now: OffsetDateTime,
    new_boot_id: String,
    current_total_tx: u64,
    current_total_rx: u64,
) -> Result<NetworkInfo, String> {
    let mut network_info = if raw_data.is_empty() {
        info!("Creating new network statistics file.");
        let next_reset_timestamp = calculate_next_reset_timestamp(network_config, now)?;
        NetworkInfo {
            config: network_config.clone(),
            boot_id: new_boot_id.clone(),
            cycle_total_tx: 0,
            cycle_total_rx: 0,
            next_reset_timestamp,
            offset_tx: i64::MIN + 2, // initial value for tx
            offset_rx: i64::MIN + 2,
        }
    } else if let Ok(info) = NetworkInfo::decode(&raw_data) {
        info!("Loaded network statistics from file.");
        info
    } else {
        warn!("Network statistics file is corrupted or in an old format. Creating a new one.");
        let (old_tx, old_rx) = if let Some((tx, rx)) = parse_old_format_for_migration(&raw_data) {
            info!("Successfully migrated traffic data from old format.");
            (tx, rx)
        } else {
            (0, 0)
        };
        let next_reset_timestamp = calculate_next_reset_timestamp(network_config, now)?;
        NetworkInfo {
            config: network_config.clone(),
            boot_id: new_boot_id.clone(),
            cycle_total_tx: old_tx,
            cycle_total_rx: old_rx,
            next_reset_timestamp,
            offset_tx: i64::MIN + 2,
            offset_rx: i64::MIN + 2,
        }
    };

    // Expire the saved cycle before applying new settings, otherwise a restart
    // with changed settings can silently carry last month's traffic forward.
    let old_config = &network_info.config;
    let schedule_changed = old_config.network_statistics_mode
        != network_config.network_statistics_mode
        || match network_config.network_statistics_mode {
            NetworkStatisticsMode::Fixed => {
                old_config.network_duration != network_config.network_duration
            }
            NetworkStatisticsMode::Natural => {
                old_config.traffic_period != network_config.traffic_period
                    || old_config.traffic_reset_day != network_config.traffic_reset_day
            }
        };
    if !schedule_changed && network_config.network_statistics_mode == NetworkStatisticsMode::Natural
    {
        let saved_reset = OffsetDateTime::from_unix_timestamp(network_info.next_reset_timestamp)
            .map_err(|e| format!("Invalid saved reset timestamp: {e}"))?;
        // Older versions saved natural boundaries at UTC midnight. Reinterpret
        // that calendar date locally before checking expiry. A boundary already
        // saved in local time is UTC midnight only when its offset is zero, in
        // which case this conversion leaves it unchanged.
        if saved_reset.time() == time::Time::MIDNIGHT {
            network_info.next_reset_timestamp = local_midnight_timestamp(saved_reset.date())?;
        }
    }
    let cycle_expired = now.unix_timestamp() >= network_info.next_reset_timestamp;
    if cycle_expired || schedule_changed {
        info!("Starting a new network statistics cycle.");
        network_info.cycle_total_tx = 0;
        network_info.cycle_total_rx = 0;
        network_info.next_reset_timestamp = calculate_next_reset_timestamp(network_config, now)?;
        network_info.offset_tx = i64::MIN;
        network_info.offset_rx = i64::MIN;
    }
    network_info.config = network_config.clone();

    let is_reboot = !new_boot_id.is_empty() && network_info.boot_id != new_boot_id;
    // A new cycle takes precedence over reboot recovery. Counters at startup
    // cannot tell which side of a missed cycle boundary traffic belongs to.
    if is_reboot && !cycle_expired && !schedule_changed {
        info!("System reboot detected. Continuing the saved traffic cycle.");
        network_info.offset_tx = network_info.cycle_total_tx as i64;
        network_info.offset_rx = network_info.cycle_total_rx as i64;
    } else if network_info.offset_tx == i64::MIN || network_info.offset_tx == i64::MIN + 2 {
        network_info.offset_tx = network_info.cycle_total_tx as i64 - current_total_tx as i64;
        network_info.offset_rx = network_info.cycle_total_rx as i64 - current_total_rx as i64;
    }
    network_info.boot_id = new_boot_id;
    network_info.update_totals(current_total_tx, current_total_rx);

    Ok(network_info)
}

/// Calculates the timestamp of the next reset event based on the configuration.
fn calculate_next_reset_timestamp(
    config: &NetworkConfig,
    now: OffsetDateTime,
) -> Result<i64, String> {
    // for fixed mode
    if config.network_statistics_mode == NetworkStatisticsMode::Fixed {
        return Ok(now.unix_timestamp() + i64::from(config.network_duration));
    }

    // Calendar boundaries are interpreted in the server's local timezone.
    let now = server_local_time(now)?;
    let period = &config.traffic_period;
    let reset_day_str = &config.traffic_reset_day;

    let mut next_reset_date = now.date();

    match period {
        TrafficPeriod::Week => {
            let target_weekday = match reset_day_str.to_lowercase().as_str() {
                "mon" | "1" => Weekday::Monday,
                "tue" | "2" => Weekday::Tuesday,
                "wed" | "3" => Weekday::Wednesday,
                "thu" | "4" => Weekday::Thursday,
                "fri" | "5" => Weekday::Friday,
                "sat" | "6" => Weekday::Saturday,
                "sun" | "7" => Weekday::Sunday,
                _ => {
                    return Err(format!(
                        "Invalid weekday '{}', must be 1-7 or mon-sun",
                        reset_day_str
                    ));
                }
            };

            let mut days_to_add = target_weekday.number_from_monday() as i64
                - now.weekday().number_from_monday() as i64;
            if days_to_add <= 0 {
                days_to_add += 7;
            }
            next_reset_date = next_reset_date + time::Duration::days(days_to_add);
        }
        TrafficPeriod::Month => {
            let reset_day = reset_day_str.parse::<u8>().map_err(|_| {
                format!(
                    "Invalid day of month '{}', must be a number 1-31",
                    reset_day_str
                )
            })?;
            if !(1..=31).contains(&reset_day) {
                return Err(format!(
                    "Invalid day of month '{}', must be 1-31",
                    reset_day
                ));
            }

            let mut target_date = now.date();
            let max_day_current_month = days_in_month(target_date.year(), target_date.month());
            let target_day = reset_day.min(max_day_current_month);

            if now.day() >= target_day {
                let (year, month) = if target_date.month() == Month::December {
                    (target_date.year() + 1, Month::January)
                } else {
                    (target_date.year(), target_date.month().next())
                };
                target_date = Date::from_calendar_date(year, month, 1).unwrap();
            }

            let max_day_target_month = days_in_month(target_date.year(), target_date.month());
            let final_day = reset_day.min(max_day_target_month);
            next_reset_date = target_date.replace_day(final_day).unwrap();
        }
        TrafficPeriod::Year => {
            let parts: Vec<&str> = reset_day_str.split('/').collect();
            if parts.len() != 2 {
                return Err(format!(
                    "Invalid date format for year reset '{}', expected 'MM/DD'",
                    reset_day_str
                ));
            }
            let month = parts[0]
                .parse::<u8>()
                .map_err(|_| format!("Invalid month in '{}'", reset_day_str))?;
            let day = parts[1]
                .parse::<u8>()
                .map_err(|_| format!("Invalid day in '{}'", reset_day_str))?;
            let month_enum = Month::try_from(month)
                .map_err(|_| format!("Invalid month value '{}', must be 1-12", month))?;

            let mut next_year = now.year();
            let reset_date_this_year = Date::from_calendar_date(next_year, month_enum, day)
                .map_err(|e| format!("Invalid date '{}/{}': {}", month, day, e))?;

            if now.date() >= reset_date_this_year {
                next_year += 1;
            }
            next_reset_date =
                Date::from_calendar_date(next_year, month_enum, day).map_err(|e| {
                    format!(
                        "Invalid date '{}/{}' for year {}: {}",
                        month, day, next_year, e
                    )
                })?;
        }
    }

    local_midnight_timestamp(next_reset_date)
}

// time::UtcOffset::local_offset_at can reject Unix multi-threaded processes.
// Linux libc's reentrant localtime_r and mktime resolve the host timezone and
// its DST rules without changing process environment variables.
fn server_local_time(now: OffsetDateTime) -> Result<OffsetDateTime, String> {
    #[cfg(unix)]
    let offset = {
        let timestamp: libc::time_t = now
            .unix_timestamp()
            .try_into()
            .map_err(|_| "Local time is outside the platform timestamp range")?;
        let mut local: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&timestamp, &mut local) }.is_null() {
            return Err("Failed to resolve server local timezone".into());
        }
        UtcOffset::from_whole_seconds(
            local
                .tm_gmtoff
                .try_into()
                .map_err(|_| "Invalid server timezone offset")?,
        )
        .map_err(|e| format!("Invalid server timezone offset: {e}"))?
    };
    #[cfg(not(unix))]
    let offset = UtcOffset::local_offset_at(now)
        .map_err(|e| format!("Failed to resolve server local timezone: {e}"))?;
    Ok(now.to_offset(offset))
}

fn local_midnight_timestamp(date: Date) -> Result<i64, String> {
    #[cfg(unix)]
    {
        let mut local: libc::tm = unsafe { std::mem::zeroed() };
        local.tm_year = date.year() - 1900;
        local.tm_mon = i32::from(date.month() as u8) - 1;
        local.tm_mday = i32::from(date.day());
        local.tm_isdst = -1; // Let libc resolve DST for the target date.
        let timestamp = unsafe { libc::mktime(&mut local) };
        if timestamp == -1 {
            return Err(format!("Failed to resolve local midnight for {date}"));
        }
        Ok(timestamp as i64)
    }
    #[cfg(not(unix))]
    {
        let midnight = date.midnight();
        let offset = server_local_time(midnight.assume_utc())?.offset();
        let offset = server_local_time(midnight.assume_offset(offset))?.offset();
        Ok(midnight.assume_offset(offset).unix_timestamp())
    }
}

/// Helper to get the number of days in a given month and year.
fn days_in_month(year: i32, month: Month) -> u8 {
    let next_month = month.next();
    let next_year = if next_month == Month::January {
        year + 1
    } else {
        year
    };
    let last_day = Date::from_calendar_date(next_year, next_month, 1)
        .unwrap()
        .previous_day()
        .unwrap();
    last_day.day()
}

/// Saves the NetworkInfo struct to the given file as a JSON string.
async fn save_network_info(file: &mut File, info: &NetworkInfo) -> Result<(), std::io::Error> {
    let content = info.encode();
    file.rewind().await?;
    file.set_len(0).await?;
    file.write_all(content.as_bytes()).await?;
    Ok(())
}

/// Gets the boot ID from the kernel. Returns an empty string on non-Linux or on error.
fn get_boot_id() -> String {
    if cfg!(target_os = "linux") {
        fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|e| {
                warn!("Failed to read boot_id: {}", e);
                String::new()
            })
    } else {
        String::new()
    }
}

/// Tries to parse the old key=value format to migrate traffic totals.
fn parse_old_format_for_migration(input: &str) -> Option<(u64, u64)> {
    let mut source_tx: Option<u64> = None;
    let mut source_rx: Option<u64> = None;

    for line in input.lines() {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "source_tx" => source_tx = value.parse().ok(),
                "source_rx" => source_rx = value.parse().ok(),
                _ => {}
            }
        }
    }

    // We need at least source_tx and source_rx. latest_tx/rx can be 0 if not present.
    if let (Some(stx), Some(srx)) = (source_tx, source_rx) {
        Some((stx, srx))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> NetworkConfig {
        NetworkConfig {
            disable_network_statistics: false,
            network_interval: 60,
            network_save_path: "unused".into(),
            traffic_period: TrafficPeriod::Month,
            traffic_reset_day: "15".into(),
            network_statistics_mode: NetworkStatisticsMode::Natural,
            network_duration: 864_000,
            network_interval_number: 6,
        }
    }

    fn date(month: Month, day: u8) -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(
            local_midnight_timestamp(Date::from_calendar_date(2026, month, day).unwrap()).unwrap(),
        )
        .unwrap()
    }

    fn saved(config: NetworkConfig) -> NetworkInfo {
        NetworkInfo {
            config,
            boot_id: "boot-1".into(),
            cycle_total_tx: 200,
            cycle_total_rx: 400,
            next_reset_timestamp: date(Month::October, 15).unix_timestamp(),
            offset_tx: -1000,
            offset_rx: -2000,
        }
    }

    #[test]
    fn first_install_excludes_traffic_before_tracking_started() {
        let info = initialize_network_info(
            &config(),
            "",
            date(Month::October, 14),
            "boot-1".into(),
            1200,
            2400,
        )
        .unwrap();
        assert_eq!((info.offset_tx, info.offset_rx), (-1200, -2400));
        assert_eq!((info.cycle_total_tx, info.cycle_total_rx), (0, 0));
    }

    #[test]
    fn sampling_settings_do_not_postpone_a_fixed_cycle() {
        let mut old_config = config();
        old_config.network_statistics_mode = NetworkStatisticsMode::Fixed;
        let old = saved(old_config.clone());
        let mut new_config = old_config;
        new_config.network_interval = 30;
        let info = initialize_network_info(
            &new_config,
            &old.encode(),
            date(Month::October, 14),
            "boot-1".into(),
            1200,
            2400,
        )
        .unwrap();
        assert_eq!(
            info.next_reset_timestamp,
            date(Month::October, 15).unix_timestamp()
        );
    }

    #[test]
    fn config_change_does_not_skip_an_overdue_cycle() {
        let old = saved(config());
        let mut new_config = config();
        new_config.network_interval_number = 3;
        let info = initialize_network_info(
            &new_config,
            &old.encode(),
            date(Month::October, 15),
            "boot-1".into(),
            1500,
            3000,
        )
        .unwrap();
        assert_eq!((info.cycle_total_tx, info.cycle_total_rx), (0, 0));
        assert_eq!((info.offset_tx, info.offset_rx), (-1500, -3000));
        assert_eq!(
            info.next_reset_timestamp,
            date(Month::November, 15).unix_timestamp()
        );
    }

    #[test]
    fn reboot_after_boundary_does_not_add_previous_cycle_bytes() {
        let old = saved(config());
        let info = initialize_network_info(
            &config(),
            &old.encode(),
            date(Month::October, 15),
            "boot-2".into(),
            100,
            200,
        )
        .unwrap();
        assert_eq!((info.offset_tx, info.offset_rx), (-100, -200));
        assert_eq!((info.cycle_total_tx, info.cycle_total_rx), (0, 0));
    }

    #[test]
    fn restart_keeps_traffic_since_last_save_and_reboot_keeps_saved_traffic() {
        let old = saved(config());
        let restarted = initialize_network_info(
            &config(),
            &old.encode(),
            date(Month::October, 14),
            "boot-1".into(),
            1300,
            2600,
        )
        .unwrap();
        assert_eq!(
            (restarted.cycle_total_tx, restarted.cycle_total_rx),
            (300, 600)
        );
        let rebooted = initialize_network_info(
            &config(),
            &old.encode(),
            date(Month::October, 14),
            "boot-2".into(),
            50,
            100,
        )
        .unwrap();
        assert_eq!(
            (rebooted.cycle_total_tx, rebooted.cycle_total_rx),
            (250, 500)
        );
    }

    #[test]
    fn interface_counter_reset_preserves_accumulated_traffic() {
        let mut info = saved(config());
        info.update_totals(1300, 2600);
        assert_eq!((info.cycle_total_tx, info.cycle_total_rx), (300, 600));
        info.update_totals(50, 100);
        assert_eq!((info.cycle_total_tx, info.cycle_total_rx), (300, 600));
        info.update_totals(70, 140);
        assert_eq!((info.cycle_total_tx, info.cycle_total_rx), (320, 640));
        let restarted = initialize_network_info(
            &config(),
            &info.encode(),
            date(Month::October, 14),
            "boot-1".into(),
            100,
            200,
        )
        .unwrap();
        assert_eq!(
            (restarted.cycle_total_tx, restarted.cycle_total_rx),
            (350, 700)
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn reset_uses_server_timezone_and_target_date_dst() {
        const CHILD: &str = "KOMARI_TEST_TRAFFIC_TIMEZONE";
        if let Ok(zone) = std::env::var(CHILD) {
            let config = config();
            let (now, expected) = if zone == "Asia/Shanghai" {
                // October 15 02:00 locally: the next reset is November 15 00:00.
                (1_792_000_800, 1_794_672_000)
            } else {
                // New York moves from UTC-4 to UTC-5 before November 15.
                (1_792_123_200, 1_794_718_800)
            };
            assert_eq!(
                calculate_next_reset_timestamp(
                    &config,
                    OffsetDateTime::from_unix_timestamp(now).unwrap()
                )
                .unwrap(),
                expected
            );
            return;
        }
        // Isolate TZ from the parallel test runner and Tokio worker threads.
        for zone in ["Asia/Shanghai", "America/New_York"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "get_info::network::network_saver::tests::reset_uses_server_timezone_and_target_date_dst", "--nocapture"])
                .env(CHILD, zone).env("TZ", zone).status().unwrap();
            assert!(status.success(), "timezone case {zone} failed");
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn legacy_utc_boundary_moves_to_local_midnight_on_upgrade() {
        const CHILD: &str = "KOMARI_TEST_TRAFFIC_MIGRATION_TIMEZONE";
        if let Ok(zone) = std::env::var(CHILD) {
            let config = config();
            let mut old = saved(config.clone());
            // The old implementation persisted October 15 00:00 UTC.
            old.next_reset_timestamp = 1_792_022_400;
            let cases: &[(i64, i64, u64)] = if zone == "Asia/Shanghai" {
                &[
                    // Local October 14 20:00: retain traffic, shorten this cycle.
                    (1_791_979_200, 1_791_993_600, 200),
                    // Local October 15 02:00: the new cycle already began.
                    (1_792_000_800, 1_794_672_000, 0),
                ]
            } else {
                // Local October 14 21:00: UTC midnight passed, local midnight has not.
                &[(1_792_026_000, 1_792_036_800, 200)]
            };
            for &(now, next_reset, total_tx) in cases {
                let info = initialize_network_info(
                    &config,
                    &old.encode(),
                    OffsetDateTime::from_unix_timestamp(now).unwrap(),
                    "boot-1".into(),
                    1200,
                    2400,
                )
                .unwrap();
                assert_eq!(info.next_reset_timestamp, next_reset);
                assert_eq!(info.cycle_total_tx, total_tx);
                assert_eq!(info.cycle_total_rx, total_tx * 2);
            }
            return;
        }
        for zone in ["Asia/Shanghai", "America/New_York"] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "get_info::network::network_saver::tests::legacy_utc_boundary_moves_to_local_midnight_on_upgrade", "--nocapture"])
                .env(CHILD, zone).env("TZ", zone).status().unwrap();
            assert!(status.success(), "migration case {zone} failed");
        }
    }

    #[test]
    fn monthly_cycle_starts_on_reset_day_and_clamps_short_months() {
        let mut config = config();
        assert_eq!(
            calculate_next_reset_timestamp(&config, date(Month::October, 14)).unwrap(),
            date(Month::October, 15).unix_timestamp()
        );
        assert_eq!(
            calculate_next_reset_timestamp(&config, date(Month::October, 15)).unwrap(),
            date(Month::November, 15).unix_timestamp()
        );
        config.traffic_reset_day = "31".into();
        assert_eq!(
            calculate_next_reset_timestamp(&config, date(Month::February, 1)).unwrap(),
            date(Month::February, 28).unix_timestamp()
        );
        assert_eq!(
            calculate_next_reset_timestamp(&config, date(Month::February, 28)).unwrap(),
            date(Month::March, 31).unix_timestamp()
        );
    }
}

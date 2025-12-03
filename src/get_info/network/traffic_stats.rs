use log::{debug, error, info, trace, warn};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// 流量统计数据
#[derive(Debug, Clone, Default)]
pub struct TrafficStats {
    /// 计费周期开始的年份
    pub cycle_year: i32,
    /// 计费周期开始的月份 (1-12)
    pub cycle_month: u32,
    /// 计费日 (每月第几号开始计费)
    pub billing_day: u32,
    /// 周期内累计上行流量 (bytes)
    pub cycle_up: u64,
    /// 周期内累计下行流量 (bytes)
    pub cycle_down: u64,
    /// 上次记录时的系统总上行流量 (用于计算增量)
    pub last_total_up: u64,
    /// 上次记录时的系统总下行流量 (用于计算增量)
    pub last_total_down: u64,
}

impl TrafficStats {
    #[cfg(target_os = "windows")]
    fn get_stats_path() -> String {
        std::env::var("PROGRAMDATA").map_or_else(
            |_| "C:\\ProgramData\\komari-monitor\\traffic_stats.dat".to_string(),
            |p| format!("{p}\\komari-monitor\\traffic_stats.dat"),
        )
    }

    #[cfg(not(target_os = "windows"))]
    fn get_stats_path() -> String {
        "/var/lib/komari-monitor/traffic_stats.dat".to_string()
    }

    /// 从文件加载统计数据，如果文件不存在则创建新的
    pub fn load_or_create(billing_day: u32) -> Self {
        let path = Self::get_stats_path();
        
        if let Some(stats) = Self::load_from_file(&path) {
            // 检查 billing_day 是否变化，如果变化则重置
            if stats.billing_day != billing_day {
                warn!("计费日已从 {} 变更为 {}，将重置流量统计", stats.billing_day, billing_day);
                let new_stats = Self::new_cycle(billing_day);
                new_stats.save();
                return new_stats;
            }
            info!("已加载流量统计: 周期 {}-{:02}-{:02}, 上行: {}, 下行: {}", 
                  stats.cycle_year, stats.cycle_month, stats.billing_day,
                  format_bytes(stats.cycle_up), format_bytes(stats.cycle_down));
            stats
        } else {
            info!("未找到流量统计文件，创建新的统计周期");
            let stats = Self::new_cycle(billing_day);
            stats.save();
            stats
        }
    }

    /// 创建新的计费周期
    fn new_cycle(billing_day: u32) -> Self {
        let (year, month, day) = current_date();
        let (cycle_year, cycle_month) = calculate_cycle_start(year, month, day, billing_day);
        
        Self {
            cycle_year,
            cycle_month,
            billing_day,
            cycle_up: 0,
            cycle_down: 0,
            last_total_up: 0,
            last_total_down: 0,
        }
    }

    /// 从文件加载
    fn load_from_file(path: &str) -> Option<Self> {
        let file = fs::File::open(path).ok()?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // 格式: cycle_year,cycle_month,billing_day,cycle_up,cycle_down,last_total_up,last_total_down
        let line = lines.next()?.ok()?;
        let parts: Vec<&str> = line.trim().split(',').collect();
        
        if parts.len() != 7 {
            error!("流量统计文件格式错误");
            return None;
        }

        Some(Self {
            cycle_year: parts[0].parse().ok()?,
            cycle_month: parts[1].parse().ok()?,
            billing_day: parts[2].parse().ok()?,
            cycle_up: parts[3].parse().ok()?,
            cycle_down: parts[4].parse().ok()?,
            last_total_up: parts[5].parse().ok()?,
            last_total_down: parts[6].parse().ok()?,
        })
    }

    /// 保存到文件
    pub fn save(&self) {
        let path = Self::get_stats_path();
        
        // 确保目录存在
        if let Some(parent) = Path::new(&path).parent()
            && let Err(e) = fs::create_dir_all(parent) {
                error!("无法创建统计目录: {e}");
                return;
            }

        let content = format!(
            "{},{},{},{},{},{},{}",
            self.cycle_year,
            self.cycle_month,
            self.billing_day,
            self.cycle_up,
            self.cycle_down,
            self.last_total_up,
            self.last_total_down
        );

        match fs::File::create(&path) {
            Ok(mut file) => {
                if let Err(e) = file.write_all(content.as_bytes()) {
                    error!("写入流量统计文件失败: {e}");
                } else {
                    trace!("流量统计已保存");
                }
            }
            Err(e) => error!("创建流量统计文件失败: {e}"),
        }
    }

    /// 更新流量统计，返回当前周期的累计流量 (up, down)
    pub fn update(&mut self, current_total_up: u64, current_total_down: u64) -> (u64, u64) {
        // 检查是否需要重置周期
        let (year, month, day) = current_date();
        let (expected_cycle_year, expected_cycle_month) = 
            calculate_cycle_start(year, month, day, self.billing_day);

        if expected_cycle_year != self.cycle_year || expected_cycle_month != self.cycle_month {
            info!(
                "进入新的计费周期: {}-{:02} -> {}-{:02}",
                self.cycle_year, self.cycle_month, expected_cycle_year, expected_cycle_month
            );
            self.cycle_year = expected_cycle_year;
            self.cycle_month = expected_cycle_month;
            self.cycle_up = 0;
            self.cycle_down = 0;
            self.last_total_up = current_total_up;
            self.last_total_down = current_total_down;
            self.save();
            return (0, 0);
        }

        // 计算增量
        // 处理系统重启或计数器溢出的情况
        let delta_up = if current_total_up >= self.last_total_up {
            current_total_up - self.last_total_up
        } else {
            // 计数器可能溢出或系统重启，使用当前值作为增量
            debug!("检测到上行流量计数器重置，当前: {}, 上次: {}", current_total_up, self.last_total_up);
            current_total_up
        };

        let delta_down = if current_total_down >= self.last_total_down {
            current_total_down - self.last_total_down
        } else {
            debug!("检测到下行流量计数器重置，当前: {}, 上次: {}", current_total_down, self.last_total_down);
            current_total_down
        };

        // 更新累计值
        self.cycle_up = self.cycle_up.saturating_add(delta_up);
        self.cycle_down = self.cycle_down.saturating_add(delta_down);
        self.last_total_up = current_total_up;
        self.last_total_down = current_total_down;

        trace!(
            "流量统计更新: 周期累计上行 {}, 周期累计下行 {}",
            format_bytes(self.cycle_up),
            format_bytes(self.cycle_down)
        );

        (self.cycle_up, self.cycle_down)
    }
}

/// 获取当前日期 (year, month, day)
#[allow(clippy::cast_possible_wrap)]
fn current_date() -> (i32, u32, u32) {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    // 简单的日期计算（考虑时区偏移，这里使用 UTC+8）
    #[cfg(not(test))]
    let secs = secs + 8 * 3600; // UTC+8
    
    let days = (secs / 86400) as i32;
    
    // 从 1970-01-01 计算日期
    let (year, month, day) = days_to_ymd(days + 719_468); // 719468 是从公元0年到1970年的天数
    
    (year, month, day)
}

/// 将天数转换为年月日 (基于 Rata Die 算法)
#[allow(clippy::cast_possible_wrap)]
fn days_to_ymd(days: i32) -> (i32, u32, u32) {
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i32 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    
    (y, m, d)
}

/// 计算当前属于哪个计费周期的开始年月
fn calculate_cycle_start(year: i32, month: u32, day: u32, billing_day: u32) -> (i32, u32) {
    if day >= billing_day {
        // 当前月的计费周期
        (year, month)
    } else {
        // 上个月的计费周期
        if month == 1 {
            (year - 1, 12)
        } else {
            (year, month - 1)
        }
    }
}

/// 格式化字节数为人类可读格式
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_cycle_start() {
        // 计费日为1号
        assert_eq!(calculate_cycle_start(2025, 12, 3, 1), (2025, 12));
        assert_eq!(calculate_cycle_start(2025, 12, 1, 1), (2025, 12));
        
        // 计费日为15号
        assert_eq!(calculate_cycle_start(2025, 12, 3, 15), (2025, 11));
        assert_eq!(calculate_cycle_start(2025, 12, 15, 15), (2025, 12));
        assert_eq!(calculate_cycle_start(2025, 12, 20, 15), (2025, 12));
        
        // 跨年
        assert_eq!(calculate_cycle_start(2025, 1, 3, 15), (2024, 12));
    }

    #[test]
    fn test_days_to_ymd() {
        // 2025-12-03 对应的天数
        let (y, m, d) = days_to_ymd(739215);
        assert_eq!((y, m, d), (2025, 12, 3));
    }
}

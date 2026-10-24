//! 服务器状态：通过已有的连接读 /proc，给终端下面的状态栏用。只支持 Linux。

use serde::Serialize;

/// 各部分之间用单独一行的 @@ 隔开
pub const COMMAND: &str = "export LC_ALL=C; head -n1 /proc/stat; echo @@; cat /proc/meminfo; echo @@; cat /proc/net/dev; echo @@; cat /proc/loadavg; echo @@; df -kP / | tail -n1";

/// 一次采样。CPU 和网络是累计值，前端用两次采样的差算出使用率和网速
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Sample {
    /// CPU 总时间和空闲时间（单位是 jiffies）
    pub cpu_total: u64,
    pub cpu_idle: u64,
    /// 内存，单位字节
    pub mem_total: u64,
    pub mem_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    /// 所有网卡（不含 lo）累计收发的字节数
    pub net_rx: u64,
    pub net_tx: u64,
    /// 根分区，单位字节
    pub disk_total: u64,
    pub disk_used: u64,
    /// 1 分钟平均负载
    pub load: f64,
}

/// 解析 COMMAND 的输出，不是 Linux（没有 /proc/stat）时返回 None
pub fn parse(out: &str) -> Option<Sample> {
    let mut parts = out.split("@@\n");
    let mut s = Sample::default();

    let cpu: Vec<u64> = parts.next()?.trim().strip_prefix("cpu ")?.split_whitespace().filter_map(|v| v.parse().ok()).collect();
    if cpu.len() < 4 {
        return None;
    }
    // user nice system idle iowait irq softirq steal guest guest_nice，guest 已经算在 user 里了
    s.cpu_total = cpu.iter().take(8).sum();
    s.cpu_idle = cpu[3] + cpu.get(4).copied().unwrap_or(0);

    let mut mem = std::collections::HashMap::new();
    for line in parts.next()?.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let kb: u64 = v.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(0);
            mem.insert(k.trim(), kb * 1024);
        }
    }
    let get = |k: &str| mem.get(k).copied().unwrap_or(0);
    s.mem_total = get("MemTotal");
    // 老内核没有 MemAvailable
    let avail = mem.get("MemAvailable").copied().unwrap_or_else(|| get("MemFree") + get("Buffers") + get("Cached"));
    s.mem_used = s.mem_total.saturating_sub(avail);
    s.swap_total = get("SwapTotal");
    s.swap_used = s.swap_total.saturating_sub(get("SwapFree"));

    for line in parts.next()?.lines() {
        let Some((name, rest)) = line.split_once(':') else { continue };
        if name.trim() == "lo" {
            continue;
        }
        let v: Vec<u64> = rest.split_whitespace().filter_map(|n| n.parse().ok()).collect();
        if v.len() >= 9 {
            s.net_rx += v[0];
            s.net_tx += v[8];
        }
    }

    s.load = parts.next().and_then(|l| l.split_whitespace().next()?.parse().ok()).unwrap_or(0.0);

    // Filesystem 1024-blocks Used Available Capacity Mounted
    if let Some(df) = parts.next() {
        let v: Vec<u64> = df.split_whitespace().skip(1).take(3).filter_map(|n| n.parse().ok()).collect();
        if v.len() == 3 {
            // 和 df 的百分比一致：已用 /（已用 + 可用），不算给 root 保留的部分
            s.disk_used = v[1] * 1024;
            s.disk_total = (v[1] + v[2]) * 1024;
        }
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux() {
        let out = "cpu  100 5 50 800 20 0 5 0 0 0\n@@\nMemTotal:        2000 kB\nMemFree:          500 kB\nMemAvailable:    1500 kB\nSwapTotal:        100 kB\nSwapFree:          40 kB\n@@\nInter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n  eth0: 1000 10 0 0 0 0 0 0 2000 20 0 0 0 0 0 0\n  eth1: 1 1 0 0 0 0 0 0 2 1 0 0 0 0 0 0\n@@\n0.52 0.40 0.30 1/200 3000\n@@\n/dev/sda1 10000 6000 3000 67% /\n";
        let s = parse(out).unwrap();
        assert_eq!((s.cpu_total, s.cpu_idle), (980, 820));
        assert_eq!((s.mem_total, s.mem_used), (2000 * 1024, 500 * 1024));
        assert_eq!((s.swap_total, s.swap_used), (100 * 1024, 60 * 1024));
        assert_eq!((s.net_rx, s.net_tx), (1001, 2002));
        assert_eq!(s.load, 0.52);
        assert_eq!((s.disk_total, s.disk_used), (9000 * 1024, 6000 * 1024));
    }

    #[test]
    fn not_linux() {
        assert_eq!(parse("head: /proc/stat: No such file or directory\n@@\n"), None);
    }
}

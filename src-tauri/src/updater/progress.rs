use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransferMeasurement {
    pub bytes_per_second: Option<u64>,
    pub eta_seconds: Option<u64>,
}

/// 使用本次进程实际接收的字节计算平均速度，避免把磁盘续传的历史字节计入瞬时速度。
pub fn calculate_transfer_measurement(
    initial_bytes: u64,
    downloaded: u64,
    total: Option<u64>,
    elapsed: Duration,
) -> TransferMeasurement {
    if elapsed < Duration::from_secs(1) || downloaded <= initial_bytes {
        return TransferMeasurement::default();
    }

    let received = downloaded - initial_bytes;
    let bytes_per_second = (received as u128 * 1_000 / elapsed.as_millis().max(1)) as u64;
    if bytes_per_second == 0 {
        return TransferMeasurement::default();
    }
    let eta_seconds = total
        .filter(|total| *total > downloaded)
        .map(|total| (total - downloaded).div_ceil(bytes_per_second));

    TransferMeasurement {
        bytes_per_second: Some(bytes_per_second),
        eta_seconds,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::calculate_transfer_measurement;

    #[test]
    fn transfer_measurement_uses_only_bytes_received_in_the_current_run() {
        let measurement = calculate_transfer_measurement(
            4 * 1024 * 1024,
            14 * 1024 * 1024,
            Some(24 * 1024 * 1024),
            Duration::from_secs(5),
        );

        assert_eq!(measurement.bytes_per_second, Some(2 * 1024 * 1024));
        assert_eq!(measurement.eta_seconds, Some(5));
    }

    #[test]
    fn transfer_measurement_waits_for_a_stable_sample() {
        let measurement = calculate_transfer_measurement(
            0,
            512 * 1024,
            Some(8 * 1024 * 1024),
            Duration::from_millis(500),
        );

        assert_eq!(measurement.bytes_per_second, None);
        assert_eq!(measurement.eta_seconds, None);
    }
}

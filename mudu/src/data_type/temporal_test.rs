#[cfg(test)]
mod tests {
    use crate::data_type::temporal::{
        MICROS_PER_DAY, MICROS_PER_SECOND, TEMPORAL_MAX_PRECISION, format_fractional_micros,
        micros_from_naive_timestamp, naive_timestamp_from_micros, parse_fixed_offset_timestamp,
        parse_naive_time, parse_naive_timestamp, precision_factor, truncate_micros,
        unix_epoch_date, unix_epoch_timestamp, utc_timestamp_from_micros, validate_precision,
    };
    use chrono::{DateTime, NaiveDate, NaiveTime, Timelike, Utc};

    #[test]
    fn unix_epoch_helpers_return_epoch() {
        assert_eq!(
            unix_epoch_date(),
            NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()
        );
        assert_eq!(
            unix_epoch_timestamp(),
            NaiveDate::from_ymd_opt(1970, 1, 1)
                .unwrap()
                .and_hms_micro_opt(0, 0, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn validate_precision_accepts_zero_to_max() {
        for p in 0..=TEMPORAL_MAX_PRECISION {
            assert!(validate_precision(p).is_ok());
        }
        assert!(validate_precision(TEMPORAL_MAX_PRECISION + 1).is_err());
        assert!(validate_precision(255).is_err());
    }

    #[test]
    fn precision_factor_matches_precision() {
        assert_eq!(precision_factor(0), MICROS_PER_SECOND);
        assert_eq!(precision_factor(1), 100_000);
        assert_eq!(precision_factor(2), 10_000);
        assert_eq!(precision_factor(3), 1_000);
        assert_eq!(precision_factor(4), 100);
        assert_eq!(precision_factor(5), 10);
        assert_eq!(precision_factor(6), 1);
        assert_eq!(precision_factor(7), 1);
        assert_eq!(precision_factor(255), 1);
    }

    #[test]
    fn truncate_micros_rounds_toward_zero_for_positive_and_negative() {
        // 12:34:56.123456 == 45_296_123_456 micros since midnight
        let micros = 45_296_123_456_i64;
        assert_eq!(truncate_micros(micros, 6), micros);
        assert_eq!(truncate_micros(micros, 3), 45_296_123_000);
        assert_eq!(truncate_micros(micros, 0), 45_296_000_000);

        // Negative values use Euclidean (floor) division, matching chrono's
        // signed-duration behavior for timestamp truncation.
        assert_eq!(truncate_micros(-1, 6), -1);
        assert_eq!(truncate_micros(-1_500_000, 0), -2_000_000);
        assert_eq!(truncate_micros(-1_500_000, 1), -1_500_000);
        assert_eq!(truncate_micros(-1_500_000, 2), -1_500_000);
        assert_eq!(truncate_micros(-1_500_000, 3), -1_500_000);
    }

    #[test]
    fn format_fractional_micros_pads_and_truncates() {
        assert_eq!(format_fractional_micros(0, 0), "");
        assert_eq!(format_fractional_micros(123_456, 0), "");
        assert_eq!(format_fractional_micros(0, 6), ".000000");
        assert_eq!(format_fractional_micros(1, 6), ".000001");
        assert_eq!(format_fractional_micros(123_456, 6), ".123456");
        assert_eq!(format_fractional_micros(123_456, 3), ".123");
        assert_eq!(format_fractional_micros(45, 2), ".00");
    }

    #[test]
    fn parse_naive_time_accepts_common_formats() {
        let t = parse_naive_time("12:34:56").unwrap();
        assert_eq!(t, NaiveTime::from_hms_opt(12, 34, 56).unwrap());

        let t = parse_naive_time("12:34:56.123456").unwrap();
        assert_eq!(
            t,
            NaiveTime::from_hms_micro_opt(12, 34, 56, 123_456).unwrap()
        );

        assert!(parse_naive_time("not-a-time").is_err());
        assert!(parse_naive_time("25:00:00").is_err());
    }

    #[test]
    fn parse_naive_timestamp_accepts_iso_and_space_separators() {
        let expected = NaiveDate::from_ymd_opt(2026, 5, 20)
            .unwrap()
            .and_hms_micro_opt(14, 30, 45, 123_456)
            .unwrap();

        assert_eq!(
            parse_naive_timestamp("2026-05-20 14:30:45.123456").unwrap(),
            expected
        );
        assert_eq!(
            parse_naive_timestamp("2026-05-20 14:30:45").unwrap(),
            expected.with_nanosecond(0).unwrap()
        );
        assert_eq!(
            parse_naive_timestamp("2026-05-20T14:30:45.123456").unwrap(),
            expected
        );
        assert_eq!(
            parse_naive_timestamp("2026-05-20T14:30:45").unwrap(),
            expected.with_nanosecond(0).unwrap()
        );

        assert!(parse_naive_timestamp("not-a-timestamp").is_err());
    }

    #[test]
    fn parse_fixed_offset_timestamp_accepts_rfc3339_and_space_separator() {
        let with_t = parse_fixed_offset_timestamp("2026-05-20T14:30:45.123456+08:00").unwrap();
        let with_space = parse_fixed_offset_timestamp("2026-05-20 14:30:45.123456+08:00").unwrap();
        assert_eq!(with_t, with_space);
        assert_eq!(with_t.timestamp_micros(), with_space.timestamp_micros());

        let plain = parse_fixed_offset_timestamp("2026-05-20T14:30:45+00:00").unwrap();
        assert_eq!(
            plain.timestamp_micros(),
            DateTime::parse_from_rfc3339("2026-05-20T14:30:45+00:00")
                .unwrap()
                .timestamp_micros()
        );

        assert!(parse_fixed_offset_timestamp("not-a-timestamp").is_err());
        assert!(parse_fixed_offset_timestamp("2026-05-20T14:30:45").is_err());
        // contains 'T' so the normalization falls through to the else branch
        assert!(parse_fixed_offset_timestamp("2026-05-20T14:30:45.123").is_err());
        // contains space and no 'T' so the normalization replaces the space
        assert!(parse_fixed_offset_timestamp("2026-05-20 14:30:45.123").is_err());
    }

    #[test]
    fn micros_from_naive_timestamp_roundtrips() {
        assert_eq!(
            micros_from_naive_timestamp(unix_epoch_timestamp()).unwrap(),
            0
        );
        assert_eq!(
            micros_from_naive_timestamp(
                NaiveDate::from_ymd_opt(1970, 1, 2)
                    .unwrap()
                    .and_hms_micro_opt(0, 0, 0, 0)
                    .unwrap()
            )
            .unwrap(),
            MICROS_PER_DAY
        );
        assert_eq!(
            micros_from_naive_timestamp(
                NaiveDate::from_ymd_opt(1969, 12, 31)
                    .unwrap()
                    .and_hms_micro_opt(0, 0, 0, 0)
                    .unwrap()
            )
            .unwrap(),
            -MICROS_PER_DAY
        );
    }

    #[test]
    fn naive_timestamp_from_micros_roundtrips() {
        assert_eq!(
            naive_timestamp_from_micros(0).unwrap(),
            unix_epoch_timestamp()
        );
        assert_eq!(
            naive_timestamp_from_micros(MICROS_PER_DAY).unwrap(),
            NaiveDate::from_ymd_opt(1970, 1, 2)
                .unwrap()
                .and_hms_micro_opt(0, 0, 0, 0)
                .unwrap()
        );
        assert!(naive_timestamp_from_micros(i64::MAX).is_err());
        assert!(naive_timestamp_from_micros(i64::MIN).is_err());
    }

    #[test]
    fn utc_timestamp_from_micros_roundtrips() {
        let epoch = utc_timestamp_from_micros(0).unwrap();
        assert_eq!(epoch, DateTime::<Utc>::UNIX_EPOCH);

        let one_micro = utc_timestamp_from_micros(1).unwrap();
        assert_eq!(one_micro.timestamp_micros(), 1);

        assert!(utc_timestamp_from_micros(i64::MAX).is_err());
        assert!(utc_timestamp_from_micros(i64::MIN).is_err());
    }
}

//! Timeline sections of a query session (stage 08 of the grid-performance plan).
//!
//! Sections are built in the same pass that produces the ordered session vector.
//! The gallery order is descending by bucket time (ADR 0004), and every member
//! of a bucket shares that time, so the month of the bucket time never increases
//! along the vector: a month forms one contiguous run and a media group is never
//! split between months.

use crate::models::AssetQuerySection;

/// Rows per section before a month is split into further sections with the
/// same label. A split only happens between buckets, so one media group larger
/// than this stays in a single section.
pub const MAX_SECTION_ROWS: usize = 5_000;

/// `yyyymm` of a UTC unix timestamp in seconds. Times at or before the epoch
/// mean "no date" (the indexer stores 0 when the file time is unavailable) and
/// map to `0`.
pub fn month_key(unix_seconds: i64) -> u32 {
    if unix_seconds <= 0 {
        return 0;
    }
    // Civil-from-days (H. Hinnant), valid for every positive i64 day count here.
    let days = unix_seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year.clamp(1, 999_999) * 100 + month) as u32
}

/// Unix seconds of the first instant of `yyyymm` (UTC); days-from-civil
/// (H. Hinnant), the inverse of `month_key` for month starts.
fn month_start(key: u32) -> i64 {
    let (year, month) = (i64::from(key / 100), i64::from(key % 100));
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    (era * 146_097 + day_of_era - 719_468) * 86_400
}

/// Accumulates sections while rows are appended in session order.
#[derive(Debug, Default)]
pub struct SectionBuilder {
    sections: Vec<AssetQuerySection>,
    rows: usize,
    last_bucket: Option<i64>,
    /// `[start, end)` seconds of the current month, so consecutive rows of one
    /// month skip the calendar arithmetic (it would cost a quarter of a 1M
    /// rebuild otherwise).
    month: Option<(i64, i64, u32)>,
}

impl SectionBuilder {
    fn month_of(&mut self, bucket_time: i64) -> u32 {
        if bucket_time <= 0 {
            return 0;
        }
        if let Some((start, end, key)) = self.month {
            if (start..end).contains(&bucket_time) {
                return key;
            }
        }
        let key = month_key(bucket_time);
        let next = if key % 100 == 12 { key + 89 } else { key + 1 };
        self.month = Some((month_start(key), month_start(next), key));
        key
    }

    /// Appends the next row of the session vector. `bucket_time` is the time
    /// the row's bucket sorts by and `bucket` identifies the bucket.
    pub fn push(&mut self, bucket_time: i64, bucket: i64) {
        let t = self.month_of(bucket_time);
        let next_bucket = self.last_bucket != Some(bucket);
        self.last_bucket = Some(bucket);
        let index = self.rows;
        self.rows += 1;
        if let Some(section) = self.sections.last_mut() {
            if section.t == t && (!next_bucket || section.count < MAX_SECTION_ROWS) {
                section.count += 1;
                return;
            }
        }
        self.sections.push(AssetQuerySection {
            t,
            start: index,
            count: 1,
        });
    }

    pub fn finish(self) -> Vec<AssetQuerySection> {
        self.sections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn ts(year: i32, month: u32, day: u32) -> i64 {
        // Days from the epoch by counting, independent of `month_key`.
        let mut days = 0i64;
        for y in 1970..year {
            days += if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                366
            } else {
                365
            };
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let lengths = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        for length in &lengths[..(month - 1) as usize] {
            days += length;
        }
        (days + i64::from(day) - 1) * DAY
    }

    #[test]
    fn month_key_matches_calendar_and_maps_missing_dates_to_zero() {
        assert_eq!(month_key(0), 0);
        assert_eq!(month_key(-1), 0);
        assert_eq!(month_key(1), 197_001);
        assert_eq!(month_key(ts(2000, 2, 29) + DAY - 1), 200_002);
        assert_eq!(month_key(ts(2000, 3, 1)), 200_003);
        assert_eq!(month_key(ts(2015, 12, 31) + DAY - 1), 201_512);
        assert_eq!(month_key(ts(2016, 1, 1)), 201_601);
        assert_eq!(month_key(ts(2100, 3, 1) - 1), 210_002);
        for year in [1971, 1999, 2024, 2038, 2400] {
            for month in 1..=12 {
                let expected = year as u32 * 100 + month;
                assert_eq!(month_key(ts(year, month, 1)), expected);
                assert_eq!(
                    month_key(ts(year, month, 1) - 1) % 100,
                    if month == 1 { 12 } else { month - 1 }
                );
            }
        }
    }

    fn build(rows: &[(i64, i64)]) -> Vec<AssetQuerySection> {
        let mut builder = SectionBuilder::default();
        for &(time, bucket) in rows {
            builder.push(time, bucket);
        }
        builder.finish()
    }

    fn section(t: u32, start: usize, count: usize) -> AssetQuerySection {
        AssetQuerySection { t, start, count }
    }

    #[test]
    fn month_start_inverts_month_key_at_every_boundary() {
        for year in [1970, 1999, 2000, 2024, 2100, 2400] {
            for month in 1..=12 {
                let key = year * 100 + month;
                let start = month_start(key);
                assert_eq!(start, ts(year as i32, month, 1), "{key}");
                if start > 0 {
                    assert_eq!(month_key(start), key);
                    assert_ne!(month_key(start - 1), key);
                }
            }
        }
        // The cache must not leak a month across a boundary in either direction.
        let mut builder = SectionBuilder::default();
        for time in [
            ts(2024, 3, 1),
            ts(2024, 3, 1) - 1,
            ts(2024, 4, 1),
            ts(2024, 2, 29),
        ] {
            builder.push(time, time);
        }
        let months: Vec<u32> = builder.finish().iter().map(|section| section.t).collect();
        assert_eq!(months, vec![202_403, 202_402, 202_404, 202_402]);
    }

    #[test]
    fn month_changes_start_sections_and_missing_dates_come_last() {
        let march = ts(2024, 3, 10);
        let february = ts(2024, 2, 1);
        let rows = [(march, 1), (march, 2), (february, 3), (0, 4), (0, 5)];
        assert_eq!(
            build(&rows),
            vec![
                section(202_403, 0, 2),
                section(202_402, 2, 1),
                section(0, 3, 2)
            ]
        );
        assert!(build(&[]).is_empty());
    }

    #[test]
    fn a_group_at_the_size_limit_stays_in_one_section() {
        let time = ts(2023, 7, 4);
        let mut rows: Vec<(i64, i64)> = (0..MAX_SECTION_ROWS as i64 - 1)
            .map(|i| (time, i))
            .collect();
        // A group of three crosses the limit and is not split.
        rows.extend([(time, -1), (time, -1), (time, -1)]);
        rows.push((time, -2));
        assert_eq!(
            build(&rows),
            vec![
                section(202_307, 0, MAX_SECTION_ROWS + 2),
                section(202_307, MAX_SECTION_ROWS + 2, 1)
            ]
        );
    }

    /// 1M synthetic rows: builder cost per row, with groups and month changes.
    #[test]
    #[ignore = "timing diagnostic: run explicitly in release mode"]
    fn one_million_rows_build_quickly() {
        let start = ts(2025, 12, 31);
        let rows: Vec<(i64, i64)> = (0..1_000_000_i64)
            .map(|i| (start - i * 300, if i % 10 < 4 { -(i / 10) - 1 } else { i }))
            .collect();
        let started = std::time::Instant::now();
        let sections = build(&rows);
        let elapsed = started.elapsed();
        assert_eq!(
            sections.iter().map(|section| section.count).sum::<usize>(),
            rows.len()
        );
        println!(
            "[sections] rows=1000000 sections={} build_ms={:.2}",
            sections.len(),
            elapsed.as_secs_f64() * 1_000.0
        );
    }

    #[test]
    fn large_months_split_into_labelled_chunks_that_cover_the_vector() {
        let time = ts(2019, 5, 5);
        let rows: Vec<(i64, i64)> = (0..12_001).map(|i| (time, i)).collect();
        let sections = build(&rows);
        assert_eq!(
            sections,
            vec![
                section(201_905, 0, MAX_SECTION_ROWS),
                section(201_905, MAX_SECTION_ROWS, MAX_SECTION_ROWS),
                section(201_905, 2 * MAX_SECTION_ROWS, 2_001)
            ]
        );
        let mut next = 0;
        for section in &sections {
            assert_eq!(section.start, next);
            next += section.count;
        }
        assert_eq!(next, rows.len());
    }
}

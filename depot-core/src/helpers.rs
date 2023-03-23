use chrono::{NaiveTime, TimeZone};
use neoncore::streams::{SeekRead, SeekWrite};
use std::fmt::{Debug, Formatter};

pub(crate) trait Ser {
    fn ser<S: SeekWrite>(&self, stream: S) -> Result<u64, std::io::Error>;
}

pub(crate) trait De {
    fn de<D: SeekRead>(stream: D) -> Result<Self, std::io::Error>
    where
        Self: Sized;
}

#[derive(Clone, Copy)]
pub struct TsWithTz {
    ts: i32,
    tz: i32,
}

impl TsWithTz {
    pub(crate) fn now() -> TsWithTz {
        let now = chrono::Local::now();
        let tz_offset = now.offset().local_minus_utc();
        let ts = now.timestamp() as i32;
        TsWithTz { ts, tz: tz_offset }
    }

    pub(crate) fn as_naive_time(&self) -> Option<NaiveTime> {
        let tz = chrono::FixedOffset::east_opt(self.tz);
        let ndt = chrono::NaiveDateTime::from_timestamp_opt(self.ts as i64, 0);
        if let (Some(tz), Some(ndt)) = (tz, ndt) {
            return Some(tz.from_utc_datetime(&ndt).time());
        }
        None
    }

    pub(crate) fn to_u64(&self) -> u64 {
        (self.ts as u64) << 32 | (self.tz as u64)
    }

    pub(crate) fn from_u64(ts: u64) -> Self {
        let tz = ts & 0xFFFFFFFF;
        let ts = ts >> 32;
        Self {
            ts: ts as i32,
            tz: tz as i32,
        }
    }
}

impl Debug for TsWithTz {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(nt) = self.as_naive_time() {
            return write!(f, "{}", nt);
        }
        write!(f, "Invalid timestamp")
    }
}

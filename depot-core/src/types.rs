use crate::helpers::{De, Ser, TsWithTz};
use crate::MAGIC;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use neoncore::streams::read::{read_format, read_lpstr};
use neoncore::streams::write::{write_lpstr, write_values};
use neoncore::streams::{AnyInt, Endianness, LPWidth, SeekRead, SeekWrite};
use std::collections::BTreeMap;
use std::io::Error;
use std::io::ErrorKind;

#[derive(Debug, Clone)]
#[readonly::make]
pub struct DepotToc {
    /// If the toc is compressed and the compression level
    pub compression_level: i32,
    /// number of entries on this toc
    pub entry_count: u64,
    /// size of the resources file as a whole
    pub size: u64,
    /// A map of resource name to offsets in the resources file
    /// a map of resource name to offset, size, and compressed size
    /// if is_compressed is false, the compressed size is set to 0
    pub entries: BTreeMap<String, EntryInfo>,
}

impl Ser for DepotToc {
    fn ser<S: SeekWrite>(&self, mut output: S) -> Result<u64, Error> {
        let vals: Vec<AnyInt> = vec![
            self.compression_level.into(),
            self.entry_count.into(),
            self.size.into(),
        ];

        let mut written = write_values(
            &mut output,
            vals.as_slice(),
            neoncore::streams::Endianness::BigEndian,
        )?;

        for (name, info) in self.entries.iter() {
            written += write_lpstr(&mut output, LPWidth::LP32, Endianness::BigEndian, name)?;
            written += info.ser(&mut output)?;
        }

        Ok(written)
    }
}

impl Default for DepotToc {
    fn default() -> Self {
        Self {
            compression_level: 0,
            entry_count: 0,
            size: 0,
            entries: BTreeMap::new(),
        }
    }
}

impl De for DepotToc {
    fn de<D: SeekRead>(mut stream: D) -> Result<Self, std::io::Error>
    where
        Self: Sized,
    {
        let format = "!Wqq";
        let read = read_format(&mut stream, format)?;

        let mut toc = DepotToc {
            compression_level: read[0].try_into().unwrap(),
            entry_count: read[1].try_into().unwrap(),
            size: read[2].try_into().unwrap(),
            entries: BTreeMap::new(),
        };
        for _ in 0..toc.entry_count {
            let name = read_lpstr(&mut stream, LPWidth::LP32, Endianness::BigEndian)?;
            let entry = EntryInfo::de(&mut stream)?;
            toc.entries.insert(name, entry);
        }

        Ok(toc)
    }
}

#[derive(Debug, Clone)]
#[readonly::make]
pub struct StreamInfo {
    pub name: String,
    pub einf: EntryInfo,
}

impl From<(String, EntryInfo)> for StreamInfo {
    fn from((name, einf): (String, EntryInfo)) -> Self {
        Self { name, einf }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DepotHeader {
    pub version: u16,
    pub toc_offset: u64,
}

impl Ser for DepotHeader {
    fn ser<S: SeekWrite>(&self, mut output: S) -> Result<u64, Error> {
        output.write_u64::<BigEndian>(MAGIC)?;
        output.write_u16::<BigEndian>(self.version)?;
        output.write_u64::<BigEndian>(self.toc_offset)?;
        Ok(0)
    }
}

impl De for DepotHeader {
    fn de<D: SeekRead>(mut stream: D) -> Result<Self, std::io::Error>
    where
        Self: Sized,
    {
        let magic = stream.read_u64::<BigEndian>()?;
        if magic != MAGIC {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "invalid magic number in depot header",
            ));
        }
        let version = stream.read_u16::<BigEndian>()?;
        let toc_offset = stream.read_u64::<BigEndian>()?;
        Ok(Self {
            version,
            toc_offset,
        })
    }
}

#[derive(Debug, Clone)]
#[readonly::make]
pub struct EntryInfo {
    pub offset: u64,
    pub size: u64,
    pub stream_size: u64,
    pub flags: u64,
    pub create_ts: TsWithTz,
    pub mod_ts: TsWithTz,
    pub hash: u64,
}

impl Ser for EntryInfo {
    fn ser<S: SeekWrite>(&self, mut output: S) -> Result<u64, Error> {
        output.write_u64::<BigEndian>(self.offset)?;
        output.write_u64::<BigEndian>(self.size)?;
        output.write_u64::<BigEndian>(self.stream_size)?;
        output.write_u64::<BigEndian>(self.flags)?;
        output.write_u64::<BigEndian>(self.create_ts.to_u64())?;
        output.write_u64::<BigEndian>(self.mod_ts.to_u64())?;
        output.write_u64::<BigEndian>(self.hash)?;
        Ok(0)
    }
}

impl De for EntryInfo {
    fn de<D: SeekRead>(mut stream: D) -> Result<Self, std::io::Error>
    where
        Self: Sized,
    {
        let format = "!qqqqqqq";
        let read = read_format(&mut stream, format)?;
        Ok(Self {
            offset: read[0].try_into().unwrap(),
            size: read[1].try_into().unwrap(),
            stream_size: read[2].try_into().unwrap(),
            flags: read[3].try_into().unwrap(),
            create_ts: TsWithTz::from_u64(read[4].try_into().unwrap()),
            mod_ts: TsWithTz::from_u64(read[5].try_into().unwrap()),
            hash: read[6].try_into().unwrap(),
        })
    }
}

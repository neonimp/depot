use crate::helpers::{hash, De, Ser, TsWithTz};
use crate::MAGIC;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use io::Error;
use neoncore::streams::read::{read_format, read_lpstr};
use neoncore::streams::write::{write_lpstr, write_values};
use neoncore::streams::{AnyInt, Endianness, LPWidth};
use seahash::SeaHasher;
use std::collections::BTreeMap;
use std::fmt::{Debug};
use std::fs::OpenOptions;
use std::hash::Hasher;
use std::io::{BufReader, Cursor, ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::{fs, io, vec};

pub trait SeekReadWrite: Read + Write + Seek {}

pub trait SeekRead: Read + Seek {}

pub trait SeekWrite: Write + Seek {}

impl<T: Read + Write + Seek> SeekReadWrite for T {}

impl<T: Read + Seek> SeekRead for T {}

impl<T: Write + Seek> SeekWrite for T {}

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

#[derive(Debug, Clone)]
pub(crate) struct DepotMetadata {
    pub header: DepotHeader,
    pub toc: DepotToc,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Copy)]
#[repr(C)]
pub enum OpenMode {
    Read,
    Write,
    ReadWrite,
}

#[readonly::make]
pub struct DepotHandle<'io> {
    metadata: DepotMetadata,
    mode: OpenMode,
    header_offset: u64,
    mt_threads: usize,
    compression_frame_size: usize,
    handle: Box<dyn 'io + SeekReadWrite>,
}

impl<'io> DepotHandle<'io> {
    pub fn new<T: SeekReadWrite + 'io>(mut handle: T, mode: OpenMode) -> Result<Self, Error> {
        let header_offset = handle.stream_position()?;
        let header = DepotHeader::de(&mut handle)?;
        handle.seek(SeekFrom::Start(header.toc_offset))?;
        let toc = DepotToc::de(&mut handle)?;

        Ok(Self {
            metadata: DepotMetadata { header, toc },
            mode,
            header_offset,
            mt_threads: 1,
            compression_frame_size: 8192,
            handle: Box::new(handle),
        })
    }

    pub fn create<T: SeekReadWrite + 'io>(mut handle: T) -> Result<Self, Error> {
        let header_offset = handle.stream_position()?;
        let header = DepotHeader {
            version: 1,
            toc_offset: !0,
        };

        let toc = Default::default();
        // write the header with a bogus toc offset
        // of !0(16Eb)
        header.ser(&mut handle)?;

        Ok(Self {
            metadata: DepotMetadata { header, toc },
            mode: OpenMode::ReadWrite,
            header_offset,
            mt_threads: 1,
            compression_frame_size: 8192,
            handle: Box::new(handle),
        })
    }

    pub fn open_file<P: AsRef<Path>>(file: P, mode: OpenMode) -> Result<Self, Error> {
        let fh = match mode {
            OpenMode::Read => fs::OpenOptions::new().read(true).open(file)?,
            OpenMode::Write => fs::OpenOptions::new().write(true).open(file)?,
            OpenMode::ReadWrite => fs::OpenOptions::new().read(true).write(true).open(file)?,
        };
        Self::new(fh, mode)
    }

    pub fn open_memory(data: &'io mut [u8], mode: OpenMode) -> Result<Self, Error> {
        let cursor = Cursor::new(data);
        Self::new(cursor, mode)
    }

    pub fn set_comp_level(&mut self, level: i32) {
        self.metadata.toc.compression_level = level;
    }

    pub fn set_mt_threads(&mut self, threads: usize) {
        self.mt_threads = threads;
    }

    pub fn set_comp_frame_size(&mut self, size: usize) {
        self.compression_frame_size = size;
    }

    pub fn add_file<P: AsRef<Path>>(&mut self, path: P, progress: Option<&mut dyn FnMut(u64, u64)>) -> Result<(), Error> {
        let path = path.as_ref();

        if self.mode == OpenMode::Read {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "cannot add file to depot in read-only mode",
            ));
        }

        if path.is_dir() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("{} is a directory", path.display()),
            ));
        }

        // check if the file exists
        if !path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("file {} does not exist", path.display()),
            ));
        } else if !path.is_file() {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                format!("{} is not a file", path.display()),
            ));
        }

        // open the file for reading
        let mut fh = OpenOptions::new().read(true).open(path)?;
        // get the file size
        let size = fh.metadata()?.len();
        // create a buffered reader
        let mut stream = BufReader::new(&mut fh);
        // get the current position in the depot
        let before = self.handle.stream_position()?;

        // zero sized files are just accounted for in the toc
        if size == 0 {
            let entry_key = path.to_string_lossy().to_string();
            // write the entry info
            let entry_info = EntryInfo {
                offset: before,
                size: 0,
                stream_size: 0,
                flags: 1,
                create_ts: TsWithTz::now(),
                mod_ts: TsWithTz::now(),
                hash: !0,
            };
            entry_info.ser(&mut self.handle)?;
            self.metadata.toc.entry_count += 1;
            self.metadata.toc.entries.insert(entry_key, entry_info);
            return Ok(());
        }

        self.add_named_sized_stream(&path.to_string_lossy(), &mut stream, size, progress)
    }

    pub fn add_named_sized_stream<R: SeekRead>(
        &mut self,
        name: &str,
        mut reader: R,
        size: u64,
        mut progress: Option<&mut dyn FnMut(u64, u64)>,
    ) -> Result<(), Error> {
        let before = self.handle.stream_position()?;

        let mut compressor = zstd::stream::Encoder::new(
            self.handle.as_mut(),
            self.metadata.toc.compression_level as i32,
        )?;

        compressor.include_checksum(true)?;
        compressor.multithread(self.mt_threads as u32)?;

        let mut buf = vec![0; self.compression_frame_size];
        let mut writen = 0;

        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            compressor.write_all(&buf[..n])?;
            writen += n;
            if let Some(progress) = &mut progress {
                progress(writen as u64, size);
            }
        }

        // finish the compression
        compressor.flush()?;
        compressor.finish()?;
        self.handle.flush()?;

        reader.seek(SeekFrom::Start(0))?;
        let hash = hash(reader);

        let entry_key = name.to_owned();
        let entry = EntryInfo {
            offset: before,
            size,
            stream_size: writen as u64,
            flags: 0,
            create_ts: TsWithTz::now(),
            mod_ts: TsWithTz::now(),
            hash: hash,
        };

        self.metadata.toc.entries.insert(entry_key, entry);
        self.metadata.toc.entry_count += 1;
        self.metadata.toc.size += size;
        Ok(())
    }

    pub fn streams(&self) -> impl Iterator<Item = (&String, &EntryInfo)> {
        self.metadata.toc.entries.iter()
    }

    pub fn get_named_stream(&self, name: &str) -> Option<StreamInfo> {
        let entry = match self.metadata.toc.entries.get(name) {
            Some(e) => e,
            None => return None,
        };

        Some((name.to_owned(), entry.clone()).into())
    }

    pub fn stream_count(&self) -> u64 {
        self.metadata.toc.entry_count
    }

    /// Extracts a stream to any SeekWrite implementor
    pub fn extract_stream<W: SeekWrite>(
        &mut self,
        stream: &StreamInfo,
        mut writer: W,
    ) -> Result<(), Error> {
        let name = stream.name.clone();
        let entry = stream.einf.clone();

        // if the entry is an empty file, just return
        if entry.flags == 1 {
            return Ok(());
        }

        self.handle.seek(SeekFrom::Start(entry.offset))?;
        let mut handle_stream = BufReader::new(&mut self.handle);

        let mut hasher = SeaHasher::new();
        let mut decompressor = zstd::stream::Decoder::new(&mut handle_stream)?;
        let mut buf = vec![0; 8192];
        let mut read = 0;
        while let Ok(n) = decompressor.read(&mut buf) {
            if read + n > entry.size as usize {
                writer.write_all(&buf[..entry.size as usize - read])?;
                break;
            }
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n])?;
            hasher.write(&buf[..n]);
            read += n;
        }

        // uncompressed size sanity check
        if writer.stream_position()? != entry.size {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "uncompressed size mismatch for {}, expect: {}, actual: {}",
                    name,
                    entry.size,
                    writer.stream_position()?
                ),
            ));
        }

        // check the hash
        let hash = hasher.finish();
        if hash != entry.hash {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "hash mismatch for {}, expect: {}, actual: {}",
                    name, entry.hash, hash
                ),
            ));
        }

        Ok(())
    }

    /// Extracts a stream to a memory buffer and returns it
    /// This is a convenience function for extract_stream
    pub fn stream_to_memory(&mut self, stream: &StreamInfo) -> Result<Vec<u8>, Error> {
        let mut buf = Vec::new();
        let mut cusor = Cursor::new(&mut buf);
        self.extract_stream(stream, &mut cusor)?;
        Ok(buf)
    }

    fn finalize(&mut self) -> Result<(), Error> {
        // seek to the end of the file
        self.handle.seek(SeekFrom::End(0))?;
        // get the toc offset
        let toc_offset = self.handle.stream_position()?;
        // write the toc
        self.metadata.toc.ser(&mut self.handle)?;
        // seek to the beginning of the file
        self.handle.seek(SeekFrom::Start(0))?;
        // update then write the header
        self.metadata.header.toc_offset = toc_offset;
        self.metadata.header.ser(&mut self.handle)?;
        Ok(())
    }

    pub fn close(mut self) -> Result<(), Error> {
        self.finalize()?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.handle.flush()?;
        Ok(())
    }

    pub fn get_toc(&self) -> DepotToc {
        self.metadata.toc.clone()
    }
}

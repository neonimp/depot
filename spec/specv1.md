# Introduction
This is the specification for the depot archive format, a simple archive format for storing and retrieving files. It is designed to be simple and easy to implement, while still being flexible enough to support a wide variety of use cases. it's intended for easy random access and streaming of files, and is designed to be used in a variety of situations, including:
- Storing files in a database
- Storage of resources in a software/game.
- Any use case where you need random in memory access to files.
- Any use case where you need to stream files from a single archive.

## Types used in this spec
`LPString` - A non null terminated string prefixed with a 32bit length.

# Format
Every field is big endian, most fields are 64bit wide.

A depot stream can happen anywhere in a file or stream like a network socket. The format is designed to be easy to parse and stream. The format outline is as follows:

## A depot header
A magic number 64bit which is the string `DEPOTARC` this is used to find a the header amidst a stream of data and to identify a possibly valid file.

A supported spec versionfor this file is a 16bit integer, this is used to identify the version of the spec this file is using. This is used to identify if the file is valid or not for a given parser.

64bit toc offset, this is the offset of the table of contents from the header.

Followed by the contents of the files in the archive.

## Table of contents
```rust
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
```

The table of contents starts with a compression level, this level is used to compress the files contained in the archive individually, followed by the number of entries in the table of contents, followed by the size of the archive both 64bit, followed by the entries in the table of contents.

## Entries
The entries are stored as in the following format:
```rust
name: LPString;
pub struct EntryInfo {
    pub offset: u64,
    pub size: u64,
    pub compressed_size: u64,
    pub flags: u64,
    pub create_ts: TsWithTz,
    pub mod_ts: TsWithTz,
    pub hash: u64,
}
```

The name is a LPString, followed by the offset of the file in the archive, followed by the size of the file, followed by the compressed size of the file, followed by the flags of the file, followed by the creation timestamp of the file, followed by the modification timestamp of the file, followed by the hash of the file.

## File contents
The file contents are stored in the following format, note that this header is only present when the TOC entry has bit flag 1 set(0x01):
```rust
pub struct FileHeader {
    pub compression_level: i32,
    pub comp
}
```

Followed by the stream of the file.

//! Shipped shared geometry (`.csg`): the PSG rows with the data stream split
//! into 64 KiB zlib chunks.
//!
//! Layout: `bcsg`, row count, chunk count, one `(compressed size, file
//! offset)` pair per chunk, the 20-byte PSG rows, then the chunks back to
//! back. Stock zlib at level 6 reproduces retail chunks byte for byte; the
//! workspace flate2 resolves to zlib-ng, whose output differs, so this calls
//! stock zlib directly.

use std::path::Path;

use rayon::prelude::*;
use rustc_hash::FxHashMap;

use crate::error::{PrevisError, Result};
use crate::psg::{self, Layout, PsgGeometry, Row, ROW_SIZE};

pub const MAGIC: &[u8; 4] = b"bcsg";
const CHUNK_SIZE: usize = 0x1_0000;
const COMPRESSION_LEVEL: i32 = 6;

fn compress(input: &[u8]) -> Vec<u8> {
    let mut length = unsafe { libz_sys::compressBound(input.len() as libz_sys::uLong) };
    let mut output = vec![0u8; length as usize];
    let status = unsafe {
        libz_sys::compress2(
            output.as_mut_ptr(),
            &mut length,
            input.as_ptr(),
            input.len() as libz_sys::uLong,
            COMPRESSION_LEVEL,
        )
    };
    assert_eq!(status, libz_sys::Z_OK, "zlib compress2 failed on a bounded buffer");
    output.truncate(length as usize);
    output
}

fn decompress(input: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut output = vec![0u8; expected];
    let mut length = expected as libz_sys::uLong;
    let status = unsafe {
        libz_sys::uncompress(output.as_mut_ptr(), &mut length, input.as_ptr(), input.len() as libz_sys::uLong)
    };
    if status != libz_sys::Z_OK {
        return Err(PrevisError::invalid(format!("CSG chunk failed to inflate (zlib status {status})")));
    }
    output.truncate(length as usize);
    Ok(output)
}

pub fn from_layout(layout: &Layout) -> Vec<u8> {
    let chunks: Vec<Vec<u8>> = layout.data.par_chunks(CHUNK_SIZE).map(compress).collect();
    let header = 12 + chunks.len() * 8 + layout.rows.len() * ROW_SIZE;
    let mut out = Vec::with_capacity(header + chunks.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(layout.rows.len() as u32).to_le_bytes());
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    let mut offset = header;
    for chunk in &chunks {
        out.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += chunk.len();
    }
    layout.encode_rows(&mut out);
    for chunk in &chunks {
        out.extend_from_slice(chunk);
    }
    out
}

pub fn build(entries: &[PsgGeometry]) -> Vec<u8> {
    from_layout(&psg::layout(entries))
}

/// Full chunks compressed together, in parallel.
const CHUNKS_PER_FLUSH: usize = 256;

/// The finished `.csg`: its hash, size, and the hash of the equivalent `.psg`.
pub struct WrittenGeometry {
    pub rows: usize,
    pub data_bytes: u64,
    pub file_bytes: u64,
    pub csg_sha256: String,
    pub psg_sha256: String,
}

/// Writes a `.csg` as its data stream grows, so the stream is never held
/// whole: full 64 KiB chunks are compressed and spooled to a scratch file
/// beside the output, and [`GeometryWriter::finish`] prefixes the header the
/// chunk sizes determine.
pub struct GeometryWriter {
    path: std::path::PathBuf,
    spool_path: std::path::PathBuf,
    spool: std::io::BufWriter<std::fs::File>,
    rows: Vec<Row>,
    pending: Vec<u8>,
    full: Vec<Vec<u8>>,
    chunk_sizes: Vec<u32>,
    data_bytes: u64,
}

impl GeometryWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let mut spool_name = path.file_name().unwrap_or_default().to_os_string();
        spool_name.push(".chunks.tmp");
        let spool_path = path.with_file_name(spool_name);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| PrevisError::io(parent.display().to_string(), e))?;
        }
        let file = std::fs::File::create(&spool_path).map_err(|e| PrevisError::io(spool_path.display().to_string(), e))?;
        Ok(Self {
            path: path.to_path_buf(),
            spool_path,
            spool: std::io::BufWriter::with_capacity(1 << 20, file),
            rows: Vec::new(),
            pending: Vec::with_capacity(CHUNK_SIZE),
            full: Vec::new(),
            chunk_sizes: Vec::new(),
            data_bytes: 0,
        })
    }

    /// Bytes in the data stream so far: the next row's offset.
    pub fn data_bytes(&self) -> u64 {
        self.data_bytes
    }

    /// Appends one unique geometry row and returns its data offset. The
    /// caller keeps the stream within the u32 offsets rows and NIFs store.
    pub fn push(&mut self, vertex_desc: u64, vertex_count: u32, triangle_count: u32, data: &[u8]) -> Result<u32> {
        let offset = u32::try_from(self.data_bytes)
            .ok()
            .filter(|offset| offset.checked_add(data.len() as u32).is_some() && data.len() <= u32::MAX as usize)
            .ok_or_else(|| PrevisError::unsupported("shared geometry exceeds the 4 GiB offset limit"))?;
        self.rows.push(Row {
            offset,
            vertex_desc,
            vertex_count,
            index_count: triangle_count * 3,
        });
        self.data_bytes += data.len() as u64;
        let mut rest = data;
        while !rest.is_empty() {
            let take = rest.len().min(CHUNK_SIZE - self.pending.len());
            self.pending.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.pending.len() == CHUNK_SIZE {
                self.full.push(std::mem::replace(&mut self.pending, Vec::with_capacity(CHUNK_SIZE)));
                if self.full.len() == CHUNKS_PER_FLUSH {
                    self.flush_full()?;
                }
            }
        }
        Ok(offset)
    }

    fn flush_full(&mut self) -> Result<()> {
        use std::io::Write;
        let compressed: Vec<Vec<u8>> = self.full.par_iter().map(|chunk| compress(chunk)).collect();
        self.full.clear();
        for chunk in compressed {
            self.chunk_sizes.push(chunk.len() as u32);
            self.spool.write_all(&chunk).map_err(|e| PrevisError::io(self.spool_path.display().to_string(), e))?;
        }
        Ok(())
    }

    /// Drops the spooled chunks without writing a `.csg`.
    pub fn abandon(self) {
        drop(self.spool);
        let _ = std::fs::remove_file(&self.spool_path);
    }

    pub fn finish(mut self) -> Result<WrittenGeometry> {
        use sha2::{Digest, Sha256};
        use std::io::{Read, Write};
        if !self.pending.is_empty() {
            self.full.push(std::mem::take(&mut self.pending));
        }
        self.flush_full()?;
        let spool_error = |e| PrevisError::io(self.spool_path.display().to_string(), e);
        self.spool.flush().map_err(spool_error)?;
        drop(self.spool);
        let header = 12 + self.chunk_sizes.len() * 8 + self.rows.len() * ROW_SIZE;
        let file_bytes = header as u64 + self.chunk_sizes.iter().map(|&s| s as u64).sum::<u64>();
        if file_bytes > u32::MAX as u64 {
            let _ = std::fs::remove_file(&self.spool_path);
            return Err(PrevisError::unsupported(format!("the CSG would be {file_bytes} bytes, past its 4 GiB chunk offsets")));
        }
        let mut head = Vec::with_capacity(header);
        head.extend_from_slice(MAGIC);
        head.extend_from_slice(&(self.rows.len() as u32).to_le_bytes());
        head.extend_from_slice(&(self.chunk_sizes.len() as u32).to_le_bytes());
        let mut offset = header as u32;
        for &size in &self.chunk_sizes {
            head.extend_from_slice(&size.to_le_bytes());
            head.extend_from_slice(&offset.to_le_bytes());
            offset += size;
        }
        let layout = Layout { rows: std::mem::take(&mut self.rows), data: Vec::new() };
        let mut row_table = Vec::with_capacity(layout.rows.len() * ROW_SIZE);
        layout.encode_rows(&mut row_table);
        head.extend_from_slice(&row_table);

        let output_error = |e| PrevisError::io(self.path.display().to_string(), e);
        let mut csg_hash = Sha256::new();
        let mut psg_hash = Sha256::new();
        psg_hash.update(psg::MAGIC);
        psg_hash.update((layout.rows.len() as u32).to_le_bytes());
        psg_hash.update(&row_table);
        let mut output = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&self.path).map_err(output_error)?);
        output.write_all(&head).map_err(output_error)?;
        csg_hash.update(&head);
        let mut spool = std::io::BufReader::with_capacity(1 << 20, std::fs::File::open(&self.spool_path).map_err(spool_error)?);
        let mut remaining = self.data_bytes;
        let mut chunk = Vec::new();
        for &size in &self.chunk_sizes {
            chunk.resize(size as usize, 0);
            spool.read_exact(&mut chunk).map_err(spool_error)?;
            output.write_all(&chunk).map_err(output_error)?;
            csg_hash.update(&chunk);
            let expected = remaining.min(CHUNK_SIZE as u64) as usize;
            let inflated = decompress(&chunk, CHUNK_SIZE)?;
            if inflated.len() != expected {
                return Err(PrevisError::invalid("a spooled CSG chunk inflated to the wrong size"));
            }
            psg_hash.update(&inflated);
            remaining -= expected as u64;
        }
        output.flush().map_err(output_error)?;
        drop(spool);
        let _ = std::fs::remove_file(&self.spool_path);
        let hex = |digest: &[u8]| digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
        Ok(WrittenGeometry {
            rows: layout.rows.len(),
            data_bytes: self.data_bytes,
            file_bytes,
            csg_sha256: hex(&csg_hash.finalize()),
            psg_sha256: hex(&psg_hash.finalize()),
        })
    }
}

fn truncated() -> PrevisError {
    PrevisError::invalid("CSG is truncated")
}

/// The PSG rows and the `(compressed size, file offset)` of each chunk.
fn header(bytes: &[u8]) -> Result<(Vec<Row>, Vec<(usize, usize)>)> {
    if bytes.get(..4) != Some(MAGIC.as_slice()) {
        return Err(PrevisError::invalid("CSG magic is missing"));
    }
    let word = |at: usize| -> Result<usize> {
        Ok(u32::from_le_bytes(bytes.get(at..at + 4).ok_or_else(truncated)?.try_into().unwrap()) as usize)
    };
    let row_count = word(4)?;
    let chunk_count = word(8)?;
    let chunks: Vec<(usize, usize)> =
        (0..chunk_count).map(|i| Ok((word(12 + i * 8)?, word(16 + i * 8)?))).collect::<Result<_>>()?;
    let rows = Layout::decode_rows(bytes.get(12 + chunk_count * 8..).ok_or_else(truncated)?, row_count)?;
    Ok((rows, chunks))
}

pub fn parse(bytes: &[u8]) -> Result<Layout> {
    let (rows, chunks) = header(bytes)?;
    let inflated: Vec<Vec<u8>> = chunks
        .par_iter()
        .map(|&(size, offset)| decompress(bytes.get(offset..offset + size).ok_or_else(truncated)?, CHUNK_SIZE))
        .collect::<Result<_>>()?;
    Ok(Layout {
        rows,
        data: inflated.concat(),
    })
}

/// A `.csg` read on demand: only the row table is decoded up front, and a
/// row's bytes are inflated from just the chunks it spans, so a planner never
/// holds the whole data stream.
pub struct SharedGeometry {
    map: memmap2::Mmap,
    rows: FxHashMap<u32, Row>,
    chunks: Vec<(usize, usize)>,
}

impl SharedGeometry {
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path).map_err(|e| PrevisError::io(path.display().to_string(), e))?;
        // SAFETY: the stages only read the sidecar; nothing writes it while the planner runs.
        let map = unsafe { memmap2::Mmap::map(&file) }.map_err(|e| PrevisError::io(path.display().to_string(), e))?;
        let (rows, chunks) = header(&map)?;
        Ok(SharedGeometry { rows: rows.into_iter().map(|row| (row.offset, row)).collect(), chunks, map })
    }

    /// The row whose data starts at `offset`.
    pub fn row(&self, offset: u32) -> Option<&Row> {
        self.rows.get(&offset)
    }

    /// The row's data bytes.
    pub fn read(&self, row: &Row) -> Result<Vec<u8>> {
        let exceeds = || PrevisError::invalid("shared geometry row exceeds its data stream");
        let (start, size) = (row.offset as usize, row.data_size());
        let mut out = Vec::with_capacity(size);
        for chunk in start / CHUNK_SIZE..(start + size).div_ceil(CHUNK_SIZE) {
            let &(compressed, offset) = self.chunks.get(chunk).ok_or_else(exceeds)?;
            let inflated = decompress(self.map.get(offset..offset + compressed).ok_or_else(truncated)?, CHUNK_SIZE)?;
            let base = chunk * CHUNK_SIZE;
            let (from, to) = (start.max(base) - base, (start + size).min(base + CHUNK_SIZE) - base);
            out.extend_from_slice(inflated.get(from..to).ok_or_else(exceeds)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_multi_chunk_streams() {
        let data: Vec<u8> = (0..CHUNK_SIZE * 2 + 100).map(|i| (i * 7 % 251) as u8).collect();
        let layout = Layout {
            rows: vec![psg::Row {
                offset: 0,
                vertex_desc: 0x0003_B000_0543_0206,
                vertex_count: 1,
                index_count: 0,
            }],
            data,
        };
        let bytes = from_layout(&layout);
        assert_eq!(&bytes[..4], MAGIC);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 3);
        let parsed = parse(&bytes).unwrap();
        assert_eq!(parsed.rows, layout.rows);
        assert_eq!(parsed.data, layout.data);
    }

    #[test]
    fn streamed_writer_matches_the_whole_layout() {
        let buffers: Vec<Vec<u8>> = [30_000usize, CHUNK_SIZE * 2 + 7, 5, CHUNK_SIZE - 5, 120_000]
            .iter()
            .enumerate()
            .map(|(n, &len)| (0..len).map(|i| ((i * (n + 3)) % 251) as u8).collect())
            .collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Test - Geometry.csg");
        let mut writer = GeometryWriter::create(&path).unwrap();
        let mut layout = Layout { rows: Vec::new(), data: Vec::new() };
        for (n, buffer) in buffers.iter().enumerate() {
            let offset = writer.push(0x0003_B000_0543_0206, n as u32 + 1, 2, buffer).unwrap();
            assert_eq!(offset as usize, layout.data.len());
            layout.rows.push(psg::Row { offset, vertex_desc: 0x0003_B000_0543_0206, vertex_count: n as u32 + 1, index_count: 6 });
            layout.data.extend_from_slice(buffer);
        }
        let written = writer.finish().unwrap();
        let expected = from_layout(&layout);
        assert_eq!(std::fs::read(&path).unwrap(), expected);
        assert_eq!(written.csg_sha256, crate::sha256_hex(&expected));
        assert_eq!(written.psg_sha256, crate::sha256_hex(&psg::encode(&layout)));
        assert_eq!((written.rows, written.data_bytes, written.file_bytes), (5, layout.data.len() as u64, expected.len() as u64));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "the chunk spool is removed");
    }

    #[test]
    fn reads_rows_spanning_chunks_on_demand() {
        let data: Vec<u8> = (0..CHUNK_SIZE * 3).map(|i| (i * 13 % 251) as u8).collect();
        let row = |offset: usize, vertex_count: u32| psg::Row {
            offset: offset as u32,
            vertex_desc: 0x0003_B000_0543_0206,
            vertex_count,
            index_count: 6,
        };
        // Within the first chunk, across the first boundary, and across two.
        let rows = vec![row(10, 4), row(CHUNK_SIZE - 20, 4), row(CHUNK_SIZE - 12, (CHUNK_SIZE as u32 + 64) / 24)];
        let layout = Layout { rows: rows.clone(), data };
        let path = std::env::temp_dir().join(format!("csg_on_demand_{}.csg", std::process::id()));
        std::fs::write(&path, from_layout(&layout)).unwrap();
        let shared = SharedGeometry::open(&path).unwrap();
        for expected in &rows {
            let found = shared.row(expected.offset).unwrap();
            assert_eq!(shared.read(found).unwrap(), layout.slice(expected).unwrap());
        }
        assert!(shared.row(11).is_none());
        drop(shared);
        let _ = std::fs::remove_file(&path);
    }
}

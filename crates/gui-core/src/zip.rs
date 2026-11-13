//! A ZIP archive of stored (uncompressed) members with UTF-8 names (PKWARE APPNOTE 6.3), written
//! here without a dependency: the archive of the NumPy export. ZIP64
//! records are written where a member or the archive passes 4 GiB, as Python's `zipfile` reads.

use std::io::{self, Write};

/// The ZIP's CRC-32 (IEEE 802.3, reflected), of a table made at compile time.
struct Crc(u32);

const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xedb8_8320 } else { c >> 1 };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

impl Crc {
    fn new() -> Crc {
        Crc(0xffff_ffff)
    }

    fn update(&mut self, data: &[u8]) {
        for &b in data {
            self.0 = CRC_TABLE[((self.0 ^ u32::from(b)) & 0xff) as usize] ^ (self.0 >> 8);
        }
    }

    fn value(&self) -> u32 {
        !self.0
    }
}

/// The 32-bit field of a size or offset: 0xFFFFFFFF where ZIP64 holds it.
const MAX32: u64 = 0xffff_ffff;

/// A ZIP archive written member after member into `out`.
pub struct ZipWriter<W: Write> {
    out: W,
    offset: u64,
    central: Vec<u8>,
    count: u64,
}

impl<W: Write> ZipWriter<W> {
    pub fn new(out: W) -> ZipWriter<W> {
        ZipWriter { out, offset: 0, central: Vec::new(), count: 0 }
    }

    /// A member of the bytes of `parts`, one after the other, stored.
    pub fn add(&mut self, name: &str, parts: &[&[u8]]) -> io::Result<()> {
        let mut crc = Crc::new();
        let mut size = 0u64;
        for p in parts {
            crc.update(p);
            size += p.len() as u64;
        }
        let crc = crc.value();
        let zip64 = size >= MAX32 || self.offset >= MAX32;
        let name = name.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| io::Error::other("a member name of 64 KiB or more"))?;
        let version: u16 = if zip64 { 45 } else { 20 };
        let size32 = if zip64 { MAX32 as u32 } else { size as u32 };
        // 1980-01-01 00:00, the earliest DOS date
        let (time, date) = (0u16, 0x21u16);
        let common = |v: &mut Vec<u8>| {
            v.extend_from_slice(&version.to_le_bytes()); // version needed
            v.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
            v.extend_from_slice(&0u16.to_le_bytes()); // stored
            v.extend_from_slice(&time.to_le_bytes());
            v.extend_from_slice(&date.to_le_bytes());
            v.extend_from_slice(&crc.to_le_bytes());
            v.extend_from_slice(&size32.to_le_bytes());
            v.extend_from_slice(&size32.to_le_bytes());
            v.extend_from_slice(&name_len.to_le_bytes());
        };

        let mut local = Vec::with_capacity(64 + name.len());
        local.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        common(&mut local);
        local.extend_from_slice(&(if zip64 { 20u16 } else { 0 }).to_le_bytes()); // extra field
        local.extend_from_slice(name);
        if zip64 {
            local.extend_from_slice(&1u16.to_le_bytes());
            local.extend_from_slice(&16u16.to_le_bytes());
            local.extend_from_slice(&size.to_le_bytes());
            local.extend_from_slice(&size.to_le_bytes());
        }

        let c = &mut self.central;
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&version.to_le_bytes()); // version made by
        common(c);
        c.extend_from_slice(&(if zip64 { 28u16 } else { 0 }).to_le_bytes()); // extra field
        c.extend_from_slice(&0u16.to_le_bytes()); // comment
        c.extend_from_slice(&0u16.to_le_bytes()); // disk
        c.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        c.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        c.extend_from_slice(&(if zip64 { MAX32 as u32 } else { self.offset as u32 }).to_le_bytes());
        c.extend_from_slice(name);
        if zip64 {
            c.extend_from_slice(&1u16.to_le_bytes());
            c.extend_from_slice(&24u16.to_le_bytes());
            c.extend_from_slice(&size.to_le_bytes());
            c.extend_from_slice(&size.to_le_bytes());
            c.extend_from_slice(&self.offset.to_le_bytes());
        }

        self.out.write_all(&local)?;
        for p in parts {
            self.out.write_all(p)?;
        }
        self.offset += local.len() as u64 + size;
        self.count += 1;
        Ok(())
    }

    /// Writes the central directory and gives the output back.
    pub fn finish(mut self) -> io::Result<W> {
        let start = self.offset;
        let length = self.central.len() as u64;
        self.out.write_all(&self.central)?;
        let zip64 = self.count >= 0xffff || start >= MAX32 || length >= MAX32;
        let mut end = Vec::new();
        if zip64 {
            let record = start + length;
            end.extend_from_slice(&0x0606_4b50u32.to_le_bytes());
            end.extend_from_slice(&44u64.to_le_bytes()); // the size of the rest of the record
            end.extend_from_slice(&45u16.to_le_bytes());
            end.extend_from_slice(&45u16.to_le_bytes());
            end.extend_from_slice(&0u32.to_le_bytes());
            end.extend_from_slice(&0u32.to_le_bytes());
            end.extend_from_slice(&self.count.to_le_bytes());
            end.extend_from_slice(&self.count.to_le_bytes());
            end.extend_from_slice(&length.to_le_bytes());
            end.extend_from_slice(&start.to_le_bytes());
            end.extend_from_slice(&0x0706_4b50u32.to_le_bytes()); // the locator
            end.extend_from_slice(&0u32.to_le_bytes());
            end.extend_from_slice(&record.to_le_bytes());
            end.extend_from_slice(&1u32.to_le_bytes());
        }
        let count16 = if zip64 { 0xffff } else { self.count as u16 };
        end.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes());
        end.extend_from_slice(&count16.to_le_bytes());
        end.extend_from_slice(&count16.to_le_bytes());
        end.extend_from_slice(&(if zip64 { MAX32 as u32 } else { length as u32 }).to_le_bytes());
        end.extend_from_slice(&(if zip64 { MAX32 as u32 } else { start as u32 }).to_le_bytes());
        end.extend_from_slice(&0u16.to_le_bytes()); // comment
        self.out.write_all(&end)?;
        self.out.flush()?;
        Ok(self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_of_the_check_string() {
        let mut c = Crc::new();
        c.update(b"123456789");
        assert_eq!(c.value(), 0xcbf4_3926);
    }
}

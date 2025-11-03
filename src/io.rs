//! I/O related constituent elements.
use crate::Result;
use byteorder::{ReadBytesExt, WriteBytesExt};
use std::io::{sink, Read, Result as IoResult, Sink, Write};

const FIRST_U32_BYTE: u32 = 0xFF000000;
const SECOND_U32_BYTE: u32 = 0x00FF0000;
const THIRD_U32_BYTE: u32 = 0x0000FF00;
const FOURTH_U32_BYTE: u32 = 0x000000FF;

/// A trait for objects which can be written to byte-oriented sinks.
pub trait WriteTo {
    /// Writes this object to the given byte-oriented sink.
    fn write_to<W: Write>(&self, writer: W) -> Result<()>;

    /// Writes this object to a given byte-oriented borrowed sink.
    fn write_to_borrowed_writer<W: Write>(&self, writer: &mut W) -> Result<()>;
}

#[derive(Debug)]
pub struct ByteCounter<T> {
    inner: T,
    count: u64,
}
impl<T> ByteCounter<T> {
    pub fn new(inner: T) -> Self {
        ByteCounter { inner, count: 0 }
    }

    pub fn count(&self) -> u64 {
        self.count
    }
}
impl ByteCounter<Sink> {
    pub fn with_sink() -> Self {
        Self::new(sink())
    }

    pub fn calculate<F>(f: F) -> Result<u64>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        let mut writer = ByteCounter::with_sink();
        track!(f(&mut writer))?;
        Ok(writer.count() as u64)
    }
}
impl<T: Write> Write for ByteCounter<T> {
    fn write(&mut self, buf: &[u8]) -> IoResult<usize> {
        let size = self.inner.write(buf)?;
        self.count += size as u64;
        Ok(size)
    }

    fn flush(&mut self) -> IoResult<()> {
        self.inner.flush()
    }
}

#[derive(Debug)]
pub(crate) struct AvcBitReader<R> {
    stream: R,
    byte: u8,
    bit_offset: usize,
}
impl<R: Read> AvcBitReader<R> {
    pub fn new(stream: R) -> Self {
        AvcBitReader {
            stream,
            byte: 0,
            bit_offset: 8,
        }
    }

    pub fn read_bit(&mut self) -> Result<u8> {
        if self.bit_offset == 8 {
            self.byte = track_io!(self.stream.read_u8())?;
            self.bit_offset = 0;
        }
        let bit = (self.byte >> (7 - self.bit_offset)) & 0b1;
        self.bit_offset += 1;
        Ok(bit)
    }

    pub fn read_byte(&mut self) -> Result<u8> {
        self.bit_offset = 0;
        self.byte = track_io!(self.stream.read_u8())?;
        Ok(self.byte)
    }

    pub fn read_ue(&mut self) -> Result<u64> {
        track!(self.read_exp_golomb_code())
    }

    pub fn read_se(&mut self) -> Result<i64> {
        let code_num = track!(self.read_exp_golomb_code())?;
        let value = i64::pow(-1, code_num as u32 + 1) * (code_num as f64 / 2.0).ceil() as i64;
        Ok(value)
    }

    fn read_exp_golomb_code(&mut self) -> Result<u64> {
        let mut leading_zeros = 0;
        while 0 == track!(self.read_bit())? {
            leading_zeros += 1;
        }
        let mut n = 0;
        for _ in 0..leading_zeros {
            let bit = track!(self.read_bit())?;
            n = (n << 1) | u64::from(bit);
        }
        n += 2u64.pow(leading_zeros) - 1;
        Ok(n)
    }
}

#[derive(Debug)]
pub(crate) struct AvcBitWriter<W> {
    stream: W,
    byte: u8,
    bit_position: usize,
}
impl<W: Write> AvcBitWriter<W> {
    pub fn new(stream: W) -> Self {
        AvcBitWriter {
            stream,
            byte: 0,
            bit_position: 0,
        }
    }

    pub fn write_bool(&mut self, value: bool) -> Result<()> {
        let byte: u8 = if value { 1 } else { 0 };
        self.write_bit(byte)
    }

    pub fn write_bit(&mut self, value: u8) -> Result<()> {
        let b = value << (8 - self.bit_position - 1);
        self.bit_position = self.bit_position + 1;
        self.byte = self.byte | b;

        if self.bit_position == 8 {
            self.bit_position = 0;
            track_io!(self.stream.write_u8(self.byte))?;
            self.byte = 0;
        }

        Ok(())
    }

    pub fn write_byte(&mut self, value: u8) -> Result<()> {
        self.bit_position = 0;
        self.byte = 0;
        track_io!(self.stream.write_u8(value))?;
        Ok(())
    }

    pub fn write_n_bits(&mut self, n: u32, value: u32) -> Result<()> {
        let bytes = [
            ((value & FIRST_U32_BYTE) >> 24) as u8,
            ((value & SECOND_U32_BYTE) >> 16) as u8,
            ((value & THIRD_U32_BYTE) >> 8) as u8,
            ((value & FOURTH_U32_BYTE) >> 0) as u8,
        ];

        let bytes_needed = (n as f64 / 8.0).ceil() as u32;
        let start_index = 4 - bytes_needed;

        for i in start_index..4 {
            let n = if i == start_index && n != 8 { n % 8 } else { 8 };

            let byte = bytes[i as usize];

            if self.bit_position == 0 && n == 8 {
                track_io!(self.stream.write_u8(byte))?;
                return Ok(());
            }

            let mut remaining_bits = n as usize;

            while remaining_bits > 0 {
                let available_bits = 8 - self.bit_position;
                let bits_to_use = std::cmp::min(available_bits, remaining_bits as usize);

                let mut mask: u8 = (2_u8.pow(bits_to_use as u32) - 1) as u8;
                let mask_shift = remaining_bits - bits_to_use;
                mask = mask << mask_shift;

                let mut push_value = (byte & mask) >> mask_shift;

                push_value = push_value << (8 - self.bit_position - bits_to_use);

                self.byte = self.byte | push_value;
                self.bit_position = self.bit_position + bits_to_use;

                if self.bit_position == 8 {
                    track_io!(self.stream.write_u8(self.byte))?;
                    self.byte = 0;
                    self.bit_position = 0;
                }

                remaining_bits = remaining_bits - bits_to_use;
            }
        }
        Ok(())
    }

    pub fn write_ue(&mut self, value: u64) -> Result<()> {
        let mut bits = 0;
        let mut cuml = 0;

        for i in 0..15 {
            if value < cuml + (1 << i) {
                bits = i;
                break;
            }
            cuml = cuml + (1 << i);
        }

        self.write_n_bits(bits, 0)?;
        self.write_bit(1)?;
        self.write_n_bits(bits, (value - cuml) as u32)?;

        Ok(())
    }

    pub fn write_se(&mut self, value: i64) -> Result<()> {
        let code_num = if value > 0 {
            (value as u64) * 2 - 1
        } else {
            (-value as u64) * 2
        };
        self.write_ue(code_num)
    }

    pub fn flush(&mut self) -> Result<()> {
        if self.bit_position > 0 {
            track_io!(self.stream.write_u8(self.byte))?;
        }

        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_by_bit_writing() {
        let expected: [u8; 2] = [0b10011010, 0b10010011];
        let mut actual = Vec::<u8>::with_capacity(2);

        let mut writer = AvcBitWriter::new(&mut actual);

        for b in expected {
            for i in 0..8 {
                let offset = 8 - i - 1;
                let mask = 1 << offset;
                let bit = (b & mask) >> offset;

                writer.write_bit(bit).unwrap();
            }
        }

        writer.flush().unwrap();

        assert_eq!(expected.len(), actual.len());

        for i in 0..expected.len() {
            assert_eq!(expected[i], actual[i]);
        }
    }

    #[test]
    fn test_ue() {
        let mut buffer = Vec::<u8>::new();
        let mut writer = AvcBitWriter::new(&mut buffer);

        for i in 0..1_001 {
            writer.write_ue(i as u64).unwrap();
        }

        writer.flush().unwrap();

        let mut reader = AvcBitReader::new(buffer.as_slice());
        for i in 0..1_001 {
            assert_eq!(reader.read_ue().unwrap(), i as u64);
        }
    }

    #[test]
    fn test_se() {
        let mut buffer = Vec::<u8>::new();
        let mut writer = AvcBitWriter::new(&mut buffer);

        for i in -500..501 {
            writer.write_se(i as i64).unwrap();
        }

        writer.flush().unwrap();

        let mut reader = AvcBitReader::new(buffer.as_slice());
        for i in -500..501 {
            assert_eq!(reader.read_se().unwrap(), i as i64);
        }
    }

    #[test]
    fn test_write_n_bits() {
        let mut buffer = Vec::<u8>::new();
        let mut writer = AvcBitWriter::new(&mut buffer);
        let expected: [u8; 2] = [0b00001111, 0b11110000];

        writer.write_n_bits(4, 0).unwrap();
        writer.write_n_bits(4, 0xFF).unwrap();

        writer.write_n_bits(4, 0xFF).unwrap();
        writer.write_n_bits(4, 0).unwrap();
        writer.flush().unwrap();

        assert_eq!(expected.len(), expected.len());

        for i in 0..buffer.len() {
            assert_eq!(expected[i], buffer[i]);
        }
    }

    #[test]
    fn test_write_n_bits_across_bounds() {
        let mut buffer = Vec::<u8>::new();
        let mut writer = AvcBitWriter::new(&mut buffer);
        let expected: [u8; 2] = [0b00000111, 0b11000000];

        writer.write_n_bits(5, 0).unwrap();
        writer.write_n_bits(5, 0xFF).unwrap();
        writer.flush().unwrap();

        assert_eq!(expected.len(), expected.len());

        for i in 0..buffer.len() {
            assert_eq!(expected[i], buffer[i]);
        }
    }
}

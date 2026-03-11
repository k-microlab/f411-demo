use core::{fmt::Debug, prelude::rust_2021::derive};
use core::ops::{Deref, DerefMut};
use byteorder::ByteOrder;

/// A std::io::Cursor like buffer interface with byteorder support and no_std
/// compatibility.
#[derive(Debug)]
pub struct Cursor<T> {
    buffer: T,
}

impl<'buffer> Cursor<&'buffer [u8]> {
    /// Constructs a new cursor object on top of a slice.
    pub fn new(buffer: &'buffer [u8]) -> Self {
        Self { buffer }
    }

    #[allow(clippy::len_without_is_empty)]
    /// Returns the length of the underlying buffer.
    pub fn len(&self) -> usize {
        self.get_buf().len()
    }

    /// Returns the underlying buffer.
    pub fn get_buf(&self) -> &'buffer [u8] {
        self.buffer
    }

    pub fn has_next(&self) -> bool {
        self.buffer.len() > 0
    }

    /// Returns the underlying buffer.
    pub fn into_inner(self) -> &'buffer [u8] {
        self.buffer
    }

    pub fn take_slice<'this, 'slice>(&'this mut self, len: usize) -> &'slice [u8] where 'buffer: 'slice {
        self.buffer.split_off(..len).expect("buffer underflow")
    }

    /// Reads data from the underlying buffer to the given slice and advances
    /// cursor position.
    /// Panics if there is not enough data remaining to fill the slice.
    pub fn read_bytes(&mut self, len: usize) -> &'buffer [u8] {
        self.buffer.split_off(..len).expect("buffer underflow")
    }

    /// Reads a 8bit integer value from the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough data remaining.
    pub fn read_u8(&mut self) -> u8 {
        *self.buffer.split_off_first().expect("buffer underflow")
    }

    /// Reads a 16bit integer value from the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough data remaining.
    pub fn read_u16<B: ByteOrder>(&mut self) -> u16 {
        let bytes = self.buffer.split_off(..2).expect("buffer underflow");
        B::read_u16(bytes)
    }

    /// Reads a 24bit integer value from the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough data remaining.
    pub fn read_u24<B: ByteOrder>(&mut self) -> u32 {
        let bytes = self.buffer.split_off(..3).expect("buffer underflow");
        B::read_u24(bytes)
    }

    /// Reads a 32bit integer value from the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough data remaining.
    pub fn read_u32<B: ByteOrder>(&mut self) -> u32 {
        let bytes = self.buffer.split_off(..4).expect("buffer underflow");
        B::read_u32(bytes)
    }

    /// Reads a 64bit integer value from the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough data remaining.
    pub fn read_u64<B: ByteOrder>(&mut self) -> u64 {
        let bytes = self.buffer.split_off(..8).expect("buffer underflow");
        B::read_u64(bytes)
    }

    /// Reads a 8bit integer value from the underlying buffer at a given
    /// offset from the cursor position without advancing the cursor position.
    /// Panics if there is not enough data remaining.
    pub fn peek_u8(&self, offset: usize) -> u8 {
        *self.buffer.get(offset).expect("buffer underflow")
    }

    /// Reads a 16bit integer value from the underlying buffer at a given
    /// offset from the cursor position without advancing the cursor position.
    /// Panics if there is not enough data remaining.
    pub fn peek_u16<B: ByteOrder>(&self, offset: usize) -> u16 {
        B::read_u16(&self.buffer[offset..])
    }

    /// Reads a 24bit integer value from the underlying buffer at a given
    /// offset from the cursor position without advancing the cursor position.
    /// Panics if there is not enough data remaining.
    pub fn peek_u24<B: ByteOrder>(&self, offset: usize) -> u32 {
        B::read_u24(&self.buffer[offset..])
    }

    /// Reads a 32bit integer value from the underlying buffer at a given
    /// offset from the cursor position without advancing the cursor position.
    /// Panics if there is not enough data remaining.
    pub fn peek_u32<B: ByteOrder>(&self, offset: usize) -> u32 {
        B::read_u32(&self.buffer[offset..])
    }
}

impl<'buffer> Deref for Cursor<&'buffer mut [u8]> {
    type Target = Cursor<&'buffer [u8]>;

    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

impl<'buffer> DerefMut for Cursor<&'buffer mut [u8]> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

impl<'buffer> Cursor<&'buffer mut [u8]> {
    /// Constructs a new cursor object on top of a slice.
    pub fn new(buffer: &'buffer mut [u8]) -> Self {
        Self { buffer }
    }

    #[allow(clippy::len_without_is_empty)]
    /// Returns the length of the underlying buffer.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the underlying buffer.
    pub fn get_buf_mut(&'buffer mut self) -> &'buffer mut [u8] {
        self.buffer
    }

    /// Returns the underlying buffer.
    pub fn into_inner_mut(self) -> &'buffer mut [u8] {
        self.buffer
    }

    pub fn take_slice_mut(&mut self, len: usize) -> &'buffer mut [u8] {
        self.buffer.split_off_mut(..len).expect("buffer overflow")
    }

    /// Writes the given slice to the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough space remaining to write the slice.
    pub fn write_bytes(&mut self, src: &[u8]) {
        let window  = self.take_slice_mut(src.len());
        window.copy_from_slice(&src);
    }

    /// Writes a 8bit integer value to the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough space remaining.
    pub fn write_u8(&mut self, val: u8) {
        let v = self.buffer.split_off_first_mut().expect("buffer overflow");
        *v = val;
    }

    /// Writes a 16bit integer value to the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough space remaining.
    pub fn write_u16<B: ByteOrder>(&mut self, val: u16) {
        let bytes = self.buffer.split_off_mut(..2).expect("buffer overflow");
        B::write_u16(bytes, val);
    }

    /// Writes a 24bit integer value to the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough space remaining.
    pub fn write_u24<B: ByteOrder>(&mut self, val: u32) {
        let bytes = self.buffer.split_off_mut(..3).expect("buffer overflow");
        B::write_u24(bytes, val);
    }

    /// Writes a 32bit integer value to the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough space remaining.
    pub fn write_u32<B: ByteOrder>(&mut self, val: u32) {
        let bytes = self.buffer.split_off_mut(..4).expect("buffer overflow");
        B::write_u32(bytes, val);
    }

    /// Writes a 64bit integer value to the underlying buffer and advances
    /// cursor position.
    /// Panics if there is not enough space remaining.
    pub fn write_u64<B: ByteOrder>(&mut self, val: u64) {
        let bytes = self.buffer.split_off_mut(..8).expect("buffer overflow");
        B::write_u64(bytes, val);
    }
}
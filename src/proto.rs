use byteorder::LittleEndian;
use byteorder_cursor::Cursor;
use defmt::Format;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use crate::varint::{VarIntRead, VarIntWrite};

#[repr(u8)]
#[derive(FromPrimitive, Format)]
pub enum WireType {
    VarInt = 0,
    Fixed64 = 1,
    Len = 2,
    Fixed32 = 5,
}

#[derive(Format)]
pub enum Wire<'a> {
    VarInt(i32),
    Fixed64(u64),
    Len(&'a [u8]),
    Fixed32(u32),
}

impl<'a> Wire<'a> {
    pub fn read(cursor: &mut Cursor<&'a [u8]>, ty: WireType) -> Wire<'a> {
        match ty {
            WireType::VarInt => Wire::VarInt(cursor.read_var_i32()),
            WireType::Fixed64 => Wire::Fixed64(cursor.read_u64::<LittleEndian>()),
            WireType::Len => {
                let len = cursor.read_var_i32() as usize;
                let pos = cursor.position();
                cursor.check_remaining(len).expect("buffer underflow");
                cursor.skip(len);
                struct Cur<'a> {
                    buf: &'a [u8],
                    pos: usize,
                }
                // We checked buffer size before
                let this: &'a Cur = unsafe { core::mem::transmute(cursor) };
                Wire::Len(&this.buf[pos..(pos + len)])
            }
            WireType::Fixed32 => Wire::Fixed32(cursor.read_u32::<LittleEndian>()),
        }
    }

    pub fn ty(&self) -> WireType {
        match self {
            Wire::VarInt(_) => WireType::VarInt,
            Wire::Fixed64(_) => WireType::Fixed64,
            Wire::Len(_) => WireType::Len,
            Wire::Fixed32(_) => WireType::Fixed32,
        }
    }

    pub fn write(&self, cursor: &mut Cursor<&'a mut [u8]>) {
        match self {
            Wire::VarInt(x) => cursor.write_var_i32(*x),
            Wire::Fixed64(x) => cursor.write_u64::<LittleEndian>(*x),
            Wire::Len(buf) => {
                cursor.write_var_i32(buf.len() as i32);
                cursor.write_bytes(buf);
            }
            Wire::Fixed32(x) => cursor.write_u32::<LittleEndian>(*x),
        }
    }

    pub fn to_var_int(self) -> Option<i32> {
        if let Wire::VarInt(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_var_int(self) -> i32 {
        self.to_var_int().expect("expected wire to be VarInt")
    }

    pub fn to_fixed64(self) -> Option<u64> {
        if let Wire::Fixed64(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_fixed64(self) -> u64 {
        self.to_fixed64().expect("expected wire to be Fixed64")
    }

    pub fn to_len(self) -> Option<&'a [u8]> {
        if let Wire::Len(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_len(self) -> &'a [u8] {
        self.to_len().expect("expected wire to be Len")
    }

    pub fn to_fixed32(self) -> Option<u32> {
        if let Wire::Fixed32(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_fixed32(self) -> u32 {
        self.to_fixed32().expect("expected wire to be Fixed32")
    }
}

pub trait ProtoRead<'a> {
    fn read_wire(&mut self) -> (usize, Wire<'a>);
}

pub trait ProtoWrite<'a> {
    fn write_wire(&mut self, id: usize, wire: Wire<'a>);
}

impl<'a> ProtoRead<'a> for Cursor<&'a [u8]> {
    fn read_wire(&mut self) -> (usize, Wire<'a>) {
        let tag = self.read_var_i32();
        let id = (tag >> 3) as usize;
        let ty = WireType::from_u8((tag & 0x07) as u8).expect("unknown wire type");
        (id, Wire::read(self, ty))
    }
}

impl<'a> ProtoWrite<'a> for Cursor<&'a mut [u8]> {
    fn write_wire(&mut self, id: usize, wire: Wire<'a>) {
        let tag = ((id as u32) << 3) | wire.ty() as u32;
        self.write_var_i32(tag as i32);
        wire.write(self);
    }
}
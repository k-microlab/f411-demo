use defmt::Format;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use crate::cursor::Cursor;
use crate::varint::{v32, VarIntRead, VarIntWrite};

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
    LenMut(&'a mut [u8]),
    Fixed32(u32),
}

impl<'buffer> Wire<'buffer> {
    pub fn read<'cursor>(cursor: &'cursor mut Cursor<&'buffer [u8]>, ty: WireType) -> Wire<'buffer> where 'buffer: 'cursor {
        match ty {
            WireType::VarInt => Wire::VarInt(cursor.read_var_i32()),
            WireType::Fixed64 => Wire::Fixed64(cursor.read_u64_le()),
            WireType::Len => {
                let len = cursor.read_var_i32() as usize;
                Wire::Len(cursor.take_slice(len))
            }
            WireType::Fixed32 => Wire::Fixed32(cursor.read_u32_le()),
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Wire::VarInt(v) => crate::varint::len_of(*v),
            Wire::Fixed64(_) => 8,
            Wire::Len(x) => {
                let len = x.len();
                crate::varint::len_of(len as i32) + len
            },
            Wire::LenMut(x) => {
                let len = x.len();
                crate::varint::len_of(len as i32) + len
            },
            Wire::Fixed32(_) => 4,
        }
    }

    pub fn ty(&self) -> WireType {
        match self {
            Wire::VarInt(_) => WireType::VarInt,
            Wire::Fixed64(_) => WireType::Fixed64,
            Wire::Len(_) => WireType::Len,
            Wire::LenMut(_) => WireType::Len,
            Wire::Fixed32(_) => WireType::Fixed32,
        }
    }

    pub fn write(&self, cursor: &mut Cursor<&'buffer mut [u8]>) {
        match self {
            Wire::VarInt(x) => cursor.write_var_i32(*x),
            Wire::Fixed64(x) => cursor.write_u64_le(*x),
            Wire::Len(buf) => {
                cursor.write_var_i32(buf.len() as i32);
                cursor.write_bytes(buf);
            }
            Wire::LenMut(buf) => {
                cursor.write_var_i32(buf.len() as i32);
                cursor.write_bytes(buf);
            }
            Wire::Fixed32(x) => cursor.write_u32_le(*x),
        }
    }

    pub fn to_var_int(self) -> Option<i32> {
        if let Wire::VarInt(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_var_int(self, field: &'static str) -> i32 {
        if let Wire::VarInt(x) = self {
            x
        } else {
            defmt::panic!("expected wire `{}` to be VarInt", field);
        }
    }

    pub fn to_fixed64(self) -> Option<u64> {
        if let Wire::Fixed64(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_fixed64(self, field: &'static str) -> u64 {
        if let Wire::Fixed64(x) = self {
            x
        } else {
            defmt::panic!("expected wire `{}` to be Fixed64", field);
        }
    }

    pub fn to_len(self) -> Option<&'buffer [u8]> {
        if let Wire::Len(x) = self {
            Some(x)
        } else if let Wire::LenMut(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_len(self, field: &'static str) -> &'buffer [u8] {
        if let Wire::Len(x) = self {
            x
        } else if let Wire::LenMut(x) = self {
            x
        } else {
            defmt::panic!("expected wire `{}` to be Len", field);
        }
    }

    pub fn to_len_mut(self) -> Option<&'buffer [u8]> {
        if let Wire::LenMut(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_len_mut(self, field: &'static str) -> &'buffer mut [u8] {
        if let Wire::LenMut(x) = self {
            x
        } else {
            defmt::panic!("expected wire `{}` to be LenMut", field);
        }
    }

    pub fn to_fixed32(self) -> Option<u32> {
        if let Wire::Fixed32(x) = self {
            Some(x)
        } else {
            None
        }
    }

    pub fn expect_fixed32(self, field: &'static str) -> u32 {
        if let Wire::Fixed32(x) = self {
            x
        } else {
            defmt::panic!("expected wire `{}` to be Fixed32", field);
        }
    }
}

pub trait ReadWire<'buffer> {
    fn read_wire<'cursor>(&'cursor mut self) -> (usize, Wire<'buffer>);
}

pub trait WriteWire<'a> {
    fn write_wire(&mut self, id: usize, wire: Wire<'a>);
}

impl<'buffer> ReadWire<'buffer> for Cursor<&'buffer [u8]> {
    fn read_wire<'cursor>(&'cursor mut self) -> (usize, Wire<'buffer>) {
        let tag = self.read_var_i32();
        let id = (tag >> 3) as usize;
        let ty = WireType::from_u8((tag & 0x07) as u8).expect("unknown wire type");
        (id, Wire::read(self, ty))
    }
}

impl<'a> WriteWire<'a> for Cursor<&'a mut [u8]> {
    fn write_wire(&mut self, id: usize, wire: Wire<'a>) {
        let tag = ((id as u32) << 3) | wire.ty() as u32;
        self.write_var_i32(tag as i32);
        wire.write(self);
    }
}

pub trait FromWire<'a>: Default + Sized {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self;
}

pub trait ToWire<'a>: Default + Sized {
    fn to_unsized_bytes(&self, cursor: &mut Cursor<&'a mut [u8]>) -> &'a mut [u8] {
        unimplemented!()
    }

    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>>;

    fn wire_len(&self) -> usize;
}

impl<'a> FromWire<'a> for bool {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        wire.expect_var_int(field) != 0
    }
}

impl<'a> ToWire<'a> for bool {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self { Some(Wire::VarInt(*self as i32)) } else { None }
    }

    fn wire_len(&self) -> usize {
        if *self { 1 } else { 0 }
    }
}

impl<'a> FromWire<'a> for u32 {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        wire.expect_fixed32(field)
    }
}

impl<'a> ToWire<'a> for u32 {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == 0 { None } else { Some(Wire::Fixed32(*self)) }
    }

    fn wire_len(&self) -> usize {
        if *self == 0 { 0 } else { 4 }
    }
}

impl<'a> FromWire<'a> for v32 {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        v32(wire.expect_var_int(field) as u32)
    }
}

impl<'a> ToWire<'a> for v32 {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if self.0 == 0 { None } else { Some(Wire::VarInt(self.0 as i32)) }
    }

    fn wire_len(&self) -> usize {
        if self.0 > 0 { crate::varint::len_of(self.0 as i32) } else { 0 }
    }
}

impl<'a> FromWire<'a> for f32 {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        f32::from_bits(wire.expect_fixed32(field))
    }
}

impl<'a> ToWire<'a> for f32 {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == 0.0 { None } else { Some(Wire::Fixed32(self.to_bits())) }
    }

    fn wire_len(&self) -> usize {
        if *self != 0.0 { 4 } else { 0 }
    }
}

impl<'a, T> FromWire<'a> for Option<T> where T: FromWire<'a> {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        Some(T::from_wire(wire, field))
    }
}

impl<'a, T> ToWire<'a> for Option<T> where T: ToWire<'a> {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        self.as_ref().map(|value| T::to_wire(value, cursor)).flatten()
    }

    fn wire_len(&self) -> usize {
        if let Some(x) = self { T::wire_len(x) } else { 0 }
    }
}

impl<'a> FromWire<'a> for u64 {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        wire.expect_fixed64(field)
    }
}

impl<'a> ToWire<'a> for u64 {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == 0 { None } else { Some(Wire::Fixed64(*self)) }
    }

    fn wire_len(&self) -> usize {
        if *self == 0 { 0 } else { 8 }
    }
}

impl<'a> FromWire<'a> for f64 {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        f64::from_bits(wire.expect_fixed64(field))
    }
}

impl<'a> ToWire<'a> for f64 {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == 0.0 { None } else { Some(Wire::Fixed64(self.to_bits())) }
    }

    fn wire_len(&self) -> usize {
        if *self != 0.0 { 8 } else { 0 }
    }
}

impl<'a> FromWire<'a> for &'a [u8] {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        wire.expect_len(field)
    }
}

impl<'a> ToWire<'a> for &'a [u8] {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if self.len() > 0 {
            Some(Wire::Len(self))
        } else {
            None
        }
    }

    fn wire_len(&self) -> usize {
        let len = self.len();
        if len > 0 {
            crate::varint::len_of(self.len() as i32) + len
        } else {
            0
        }
    }
}

impl<'a> FromWire<'a> for &'a str {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        core::str::from_utf8(wire.expect_len(field)).expect("invalid UTF-8 sequence")
    }
}

impl<'a> ToWire<'a> for &'a str {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        self.as_bytes().to_wire(cursor)
    }

    fn wire_len(&self) -> usize {
        self.as_bytes().wire_len()
    }
}

#[macro_export]
macro_rules! ordinal {
    ($name:ident) => {
        impl<'a> $crate::proto::FromWire<'a> for $name {
            fn from_wire(wire: $crate::proto::Wire<'a>, field: &'static str) -> Self {
                Self::from_i32(wire.expect_var_int(field)).expect("unknown enum variant")
            }
        }

        impl<'a> $crate::proto::ToWire<'a> for $name {
            fn to_wire(&self, _cursor: &mut $crate::cursor::Cursor<&'a mut [u8]>) -> Option<$crate::proto::Wire<'a>> {
                if *self == Self::default() { None } else { Some(Wire::VarInt(*self as i32)) }
            }

            fn wire_len(&self) -> usize {
                if *self == Self::default() { 0 } else { $crate::varint::len_of(*self as i32) }
            }
        }
    };
}

#[macro_export]
macro_rules! proto {
    ($sv:vis struct $name:ident $(<$lt:lifetime>)? {
        $($(#[$attr:meta])* $fv:vis $field:ident: $ty:ty = $id: literal),* $(,)?
    }) => {
        #[derive(Default, defmt::Format)]
        $sv struct $name $(<$lt>)? {
            $($(#[$attr])* $fv $field: $ty),*
        }

        impl<'a> crate::proto::FromWire<'a> for $name $(<$lt>)? {
            #[allow(deprecated)]
            fn from_wire(wire: crate::proto::Wire<'a>, field: &'static str) -> Self {
                let payload = wire.expect_len(field);
                let mut cursor = Cursor::<&'a [u8]>::new(payload);
                let mut this = Self::default();
                while cursor.has_next() {
                    let (id, wire) = cursor.read_wire();
                    match id {
                        $($id => this.$field = <$ty>::from_wire(wire, stringify!($field)),)*
                        _ => defmt::panic!("unknown proto field #{} in {}: {}", id, field, wire),
                    }
                }
                this
            }
        }

        impl<'a> crate::proto::ToWire<'a> for $name $(<$lt>)? {
            #[allow(deprecated)]
            fn to_unsized_bytes(&self, cursor: &mut crate::cursor::Cursor<&'a mut [u8]>) -> &'a mut [u8] {
                let len = self.wire_len();
                let payload = cursor.take_slice_mut(len);
                let mut cur = crate::cursor::Cursor::<&mut [u8]>::new(&mut *payload);
                $({
                    if let Some(wire) = self.$field.to_wire(cursor) {
                        defmt::info!("writing wire #{}: {} = {} ({})", $id, stringify!($field), self.$field, wire);
                        cur.write_wire($id, wire);
                    }
                })*
                payload
            }

            #[allow(deprecated)]
            fn to_wire(&self, cursor: &mut crate::cursor::Cursor<&'a mut [u8]>) -> Option<crate::proto::Wire<'a>> {
                use crate::varint::VarIntWrite;

                let mut len = self.wire_len();
                len += crate::varint::len_of(len as i32);
                let payload = cursor.take_slice_mut(len);
                let mut cur = crate::cursor::Cursor::<&mut [u8]>::new(&mut *payload);
                cur.write_var_i32(len as i32);
                $({
                    if let Some(wire) = self.$field.to_wire(cursor) {
                        defmt::info!("writing wire #{}: {} = {} ({})", $id, stringify!($field), self.$field, wire);
                        cur.write_wire($id, wire);
                    }
                })*
                Some(crate::proto::Wire::LenMut(payload))
            }

            #[allow(deprecated)]
            fn wire_len(&self) -> usize {
                let mut len = 0;
                $({
                    let l = self.$field.wire_len();
                    if l > 0 {
                        len += l;
                        len += 1; // TAG
                    }
                })*
                defmt::info!("wire_len of {}: {}", stringify!($name), len);
                len
            }
        }
    };
}
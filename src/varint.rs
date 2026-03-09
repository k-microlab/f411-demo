use byteorder_cursor::Cursor;

trait VarIntRead {
    fn read_var_i32(&mut self) -> i32;
}

trait VarIntWrite {
    fn write_var_i32(&mut self, value: i32);
}

impl VarIntRead for Cursor<&[u8]> {
    fn read_var_i32(&mut self) -> i32 {
        let mut x = 0i32;

        for shift in [0u32, 7, 14, 21, 28].iter() { // (0..32).step_by(7)
            let b = self.read_u8() as i32;
            x |= (b & 0x7F) << *shift;
            if (b & 0x80) == 0 {
                return x;
            }
        }
        defmt::panic!("VarInt too big");
    }
}

impl VarIntWrite for Cursor<&mut [u8]> {
    fn write_var_i32(&mut self, value: i32) {
        let mut temp = value as u32;
        loop {
            if (temp & !0x7fu32) == 0 {
                self.write_u8(temp as u8);
                return;
            } else {
                self.write_u8(((temp & 0x7F) | 0x80) as u8);
                temp >>= 7;
            }
        }
    }
}
use boltffi::export;

use crate::Cursor;

#[export(single_threaded)]
impl Cursor {
    pub fn rewind(&mut self) {
        self.position = 0;
    }
}

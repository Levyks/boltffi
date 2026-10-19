use boltffi::export;

use crate::Cursor;

#[export(methods)]
impl Cursor {
    pub fn skip(&mut self) {
        self.position += 1;
    }
}

#[export(sideways)]
impl Cursor {
    pub fn back(&mut self) {
        self.position -= 1;
    }
}

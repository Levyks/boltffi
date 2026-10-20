use boltffi::export;

#[export(single_threaded)]
impl crate::Cursor {
    pub fn rewind(&mut self) {
        self.position = 0;
    }
}

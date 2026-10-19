use boltffi::{data, export};

#[data]
#[export]
#[derive(Clone, Copy)]
pub struct Both {
    pub x: u32,
}

#[export]
#[data]
#[derive(Clone, Copy)]
pub struct Either {
    pub x: u32,
}

#[repr(C)]
#[data]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[repr(u8)]
#[data]
pub enum Mode {
    Fast = 1,
    Slow = 2,
}

#[export]
pub fn echo_point(point: Point) -> Point {
    point
}

#[export]
pub fn echo_mode(mode: Mode) -> Mode {
    mode
}

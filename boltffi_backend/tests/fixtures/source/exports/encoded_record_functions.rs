#[repr(u8)]
#[data]
pub enum Role {
    Admin = 1,
    Guest = 2,
}

#[data]
pub struct User {
    pub name: String,
    pub age: u32,
    pub role: Role,
    pub nickname: Option<String>,
    pub tags: Vec<String>,
}

#[export]
pub fn echo_user(user: User) -> User {
    user
}

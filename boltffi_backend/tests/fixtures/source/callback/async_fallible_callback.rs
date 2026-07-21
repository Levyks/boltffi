#[export]
pub trait Listener {
    async fn load(&self, key: u32) -> String;
    async fn try_load(&self, key: u32) -> Result<String, String>;
}

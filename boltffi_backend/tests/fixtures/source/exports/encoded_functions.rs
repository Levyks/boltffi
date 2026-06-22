#[data]
pub struct Person {
    pub name: String,
}

#[data]
pub enum Shape {
    Label(String),
}

#[export]
pub fn keep_person(person: Person) -> Person {
    person
}

#[export]
pub fn keep_shape(shape: Shape) -> Shape {
    shape
}

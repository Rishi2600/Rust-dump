struct CustomSmartPointer {
    data: String,
}

impl Drop for CustomSmartPointer {
    fn drop(&mut self) {
        println!("Cleaning up pointer with data `{}`!", self.data);
    }
}

fn main() {
    let _c = CustomSmartPointer {
        data: String::from("resource"),
    };
    println!("Pointer created.");
    // "Cleaning up pointer..." will automatically run at the end of scope
}
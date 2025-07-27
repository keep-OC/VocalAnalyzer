pub struct Analyzer {
    device_name: String,
    count: u64,
}

impl Analyzer {
    pub fn new(device_name: String) -> Self {
        Self {
            device_name,
            count: 0,
        }
    }
    pub fn periodic(&mut self) {
        println!("{}, {}", self.device_name, self.count);
        self.count += 1;
    }
}

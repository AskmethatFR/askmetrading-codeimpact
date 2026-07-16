use std::fs::File;
use std::io::Read;

fn read_repeated(path: &str) {
    let mut file = File::open(path).unwrap();
    let mut buf = String::new();
    for _ in 0..3 {
        file.read_to_string(&mut buf).unwrap();
    }
}

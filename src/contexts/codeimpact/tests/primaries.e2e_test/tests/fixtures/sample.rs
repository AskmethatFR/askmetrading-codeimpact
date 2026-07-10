fn main() {
    let x = 42;
    if x > 0 {
        println!("positive");
    } else if x < 0 {
        println!("negative");
    } else {
        println!("zero");
    }

    for i in 0..3 {
        if i % 2 == 0 {
            println!("even: {}", i);
        }
    }
}

fn read_files() {
    let files = vec!["a.txt", "b.txt"];
    for f in &files {
        let _ = std::fs::read_to_string(f);
    }
}
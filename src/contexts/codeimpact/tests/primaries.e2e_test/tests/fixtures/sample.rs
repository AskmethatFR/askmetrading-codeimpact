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
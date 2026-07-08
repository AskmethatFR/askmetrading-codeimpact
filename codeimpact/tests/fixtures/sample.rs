fn calculate_fibonacci(n: u32) -> u32 {
    if n <= 1 {
        return n;
    }
    let mut a = 0;
    let mut b = 1;
    for _ in 2..=n {
        let temp = a + b;
        a = b;
        b = temp;
    }
    b
}

fn classify_number(x: i32) -> &'static str {
    if x > 0 {
        if x % 2 == 0 {
            "positive even"
        } else {
            "positive odd"
        }
    } else if x < 0 {
        "negative"
    } else {
        "zero"
    }
}

fn main() {
    let n = 10;
    let result = calculate_fibonacci(n);
    println!("Fibonacci({}) = {}", n, result);
    let classification = classify_number(result as i32);
    println!("Classification: {}", classification);
}
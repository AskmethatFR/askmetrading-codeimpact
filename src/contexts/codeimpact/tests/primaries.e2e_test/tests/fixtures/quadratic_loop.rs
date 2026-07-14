fn process_items(items: &[u32]) {
    for item in items {
        validate(item);
    }
}

fn validate(item: &u32) {
    for _ in 0..*item {
        println!("checking {}", item);
    }
}

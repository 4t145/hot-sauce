use hot_sauce::Hot;
#[test]
fn test_multi_thread() {
    use std::thread;
    let source = Hot::<str>::new("hello world");
    for _ in 0..1 {
        let mut message = source.clone();
        thread::spawn(move || {
            let mut version = 0;
            loop {
                version += 1;
                message.update(format!("hello world {}", version));
            }
        });
    }
    let mut message = source.clone();
    for _ in 0..10 {
        thread::sleep(std::time::Duration::from_millis(100));
        message.sync();
        // println!("{}", &*message);
    }
}

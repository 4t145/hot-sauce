# Hot-Sauce
It's wrapper to allow you dynamic update some data like config. 

You can sync the data to the newest version or get cached data of last sync, or publish a data.


```bash
cargo add hot-sauce
```

## Usage
```rust
pub fn main() {
    use std::thread;
    let source = HotSource::<str>::new("hello world");
    let mut message = source.get();
    thread::spawn(move || {
        let mut version = 0;
        loop {
            thread::sleep(std::time::Duration::from_millis(100));
            version += 1;
            message.update(format!("hello world {}", version));
        }
    });
    let mut message = source.get();
    for _ in 0..10 {
        thread::sleep(std::time::Duration::from_millis(50));
        message.sync();
        println!("{}", &**message);
    }
}
```
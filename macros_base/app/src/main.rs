macro_rules! time_it {
    ($name:expr, $block:block) => {
        {
            let start = std::time::Instant::now();
            let res = $block;
            println!("Benchmark [{}] took: {:?}", $name, start.elapsed());
            res
        }
    };
}
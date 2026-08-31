use std::{path::Path, time::Instant};

use todo_core::{parse, Projection};

fn main() {
    let mut source =
        String::from("---\ntodo-spec: 1\ncolumns:\n  - {id: todo, name: Todo}\n---\n\n## Todo\n");
    for index in 0..1000 {
        source.push_str(&format!(
            "- [ ] task-{index} <!--t:{index:06}--> #bench @runner 📅2026-09-10 !2 ^{index}\n"
        ));
    }
    let document = parse(source).expect("fixture parses");
    let mut samples = Vec::new();
    for _ in 0..20 {
        let mut projection = Projection::open_in_memory().expect("SQLite opens");
        let started = Instant::now();
        projection
            .rebuild(&[(Path::new("TODO.md"), &document)])
            .expect("projection rebuilds");
        samples.push(started.elapsed());
    }
    samples.sort();
    println!(
        "1000-task rebuild: median={:.3}ms p95={:.3}ms max={:.3}ms ({} samples)",
        samples[samples.len() / 2].as_secs_f64() * 1000.0,
        samples[samples.len() * 19 / 20].as_secs_f64() * 1000.0,
        samples.last().unwrap().as_secs_f64() * 1000.0,
        samples.len()
    );
}

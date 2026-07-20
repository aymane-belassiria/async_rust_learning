use std::thread;
use std::sync::Arc;
pub fn shared_with_arc(){
    let data = Arc::new(vec![1,2,3]);
    let mut handles = vec![];
    for i in 0..3{
        let data = Arc::clone(&data);
        handles.push(thread::spawn(move||println!("Thread {i} sees sum: {}", data.iter().sum::<i32>())));
    }
    for h in handles { h.join().unwrap();}
}
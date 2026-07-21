use std::thread;
use std::sync::{Arc, Mutex};
pub fn shared_with_arc(){
    let data = Arc::new(vec![1,2,3]);
    let mut handles = vec![];
    for i in 0..3{
        let data = Arc::clone(&data);
        handles.push(thread::spawn(move||println!("Thread {i} sees sum: {}", data.iter().sum::<i32>())));
    }
    for h in handles { h.join().unwrap();}
}

pub fn shared_with_mutex(){
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];
    for _ in 0..9{
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move ||{
            let mut num = counter.lock().unwrap();
            *num += 1; 
        }));
    }
    for h in handles { h.join().unwrap(); }
    println!("Final count: {}", counter.lock().unwrap());
}
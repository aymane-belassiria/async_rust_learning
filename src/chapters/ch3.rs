use std::sync::mpsc;
use std::thread;

pub fn channel_init(){
    let (tx, rx) = mpsc::channel();
    thread::spawn(move||{
        for i in 0..5{
            tx.send(i).unwrap();
        }
    });
    
    for received in rx{
        println!("Got: {}", received);
    }

}

pub fn channel_multiple_producers(){
    let (tx, rx) = mpsc::channel();
    for i in 0..3{
        let tx = tx.clone();
        thread::spawn(move||{tx.send(format!("hello from {i}")).unwrap();});
    }
    drop(tx);
    for msg in rx {println!("{msg}");}
}
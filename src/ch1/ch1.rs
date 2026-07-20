use std::thread;
pub fn create_thread(){
    let handle = thread::spawn(||{
        println!("Hello from a thread!");
        42
    });
    println!("Hello from the main thread!");
    let result = handle.join().unwrap();
    println!("Thread returned:{}", result);
}

pub fn thread_move(){
    let name: String = String::from("Aymane");
    //try to remove move and say what the compiler says
    thread::spawn(move ||{
        println!("{}", name);
    });
}

pub fn arbitrary_threads_exec(){
    for i in 0..5{
        thread::spawn(move||println!("Thread number: {}", i));
    }
}

pub fn scoped_threads(){
    let data:Vec<u8> = vec![1,2,3];
    thread::scope(|s|{
        s.spawn(||println!("first: {}", data[0]));
        s.spawn(||println!("second: {}", data[1]));
        s.spawn(||println!("third: {}", data[2]));
    });
    println!("data: {:?}", data);
}
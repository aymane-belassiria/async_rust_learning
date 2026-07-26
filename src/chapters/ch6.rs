use tokio::{sync::{mpsc, oneshot}, time::{Duration, sleep}};
pub async fn tokio_spawn(){
    let handle = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        "task done"
    });
    println!("main keeps going");
    let result = handle.await.unwrap();
    println!("{result}");
}

async fn fetch(id: u8) -> String{
    tokio::time::sleep(Duration::from_millis(100 * (id as u64 % 4 + 1))).await;
    format!("result-{id}")
}

pub async fn tokio_join_spawn_select(){
    //join
    let (a, b, _c) = tokio::join!(
        fetch(1),
        fetch(2),
        fetch(3),
    );

    println!("join: {a}, {b}, {b}");

    //joinSet and spawn
    let mut set = tokio::task::JoinSet::new();
    for id in 1..=10{
        set.spawn(fetch(id));
    }

    while let Some(res) = set.join_next().await{
        println!("{:?}", res.unwrap());
    }

    //select liek go select
    tokio::select! {
        result = fetch(1) => println!("got {result}"),
        _ = sleep(Duration::from_secs(2)) => println!("time out"),
    }
}

pub async fn tokio_channels(){
    let (tx, mut rx) = mpsc::channel::<String>(32);
    tokio::spawn(async move{
        tx.send("job 1".into()).await.unwrap();
    });
    while let Some(msg) = rx.recv().await{
        println!("got {msg}");
    }

    //one sender one receiver

    let (tx, rx) = oneshot::channel::<u32>();
    tokio::spawn(async move{
        let _ = tx.send(42);
    });
    let answer = rx.await.unwrap();
    println!("recieved: {answer}");
}
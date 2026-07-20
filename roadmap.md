# Async Rust & Multithreading — A Practical Guide

*Written for someone who knows Rust syntax but is new to concurrency. Goal: enough understanding to read Zero2Prod without getting lost, plus exercises to make it stick.*

---

## Table of Contents

1. [The Big Picture: Two Kinds of Concurrency](#1-the-big-picture)
2. [Part I — OS Threads (std::thread)](#2-part-i--os-threads)
3. [Part II — Sharing Data Between Threads](#3-part-ii--sharing-data)
4. [Part III — Message Passing (Channels)](#4-part-iii--channels)
5. [Part IV — What Is a Future, Really?](#5-part-iv--futures)
6. [Part V — async/await Syntax](#6-part-v--asyncawait)
7. [Part VI — Tokio, the Runtime](#7-part-vi--tokio)
8. [Part VII — Common Errors & Pitfalls](#8-part-vii--pitfalls)
9. [Part VIII — How This Maps to Zero2Prod](#9-part-viii--zero2prod)
10. [Exercises](#10-exercises)
11. [Solutions & Hints](#11-solutions)
12. [How to Read the Flitton *Async Rust* Book](#12-how-to-read-the-flitton-async-rust-book)
13. [The 7-Day Study Plan](#13-the-7-day-study-plan)

---

## 1. The Big Picture

There are two fundamentally different ways to "do many things at once":

### OS Threads (multithreading)
The **operating system** runs multiple threads, switching between them whenever it wants (*preemptive* scheduling). Each thread has its own stack (~8MB reserved). Good for **CPU-bound** work: image processing, computing hashes, parsing large files.

```
Thread 1: ████████░░░░████████
Thread 2: ░░░░████████░░░░████   ← OS decides when to switch
```

### Async Tasks (async/await)
**Your program** runs many tasks on few threads. A task voluntarily gives up control when it would otherwise wait (*cooperative* scheduling) — at every `.await`. Tasks are tiny (a few hundred bytes vs 8MB). Good for **IO-bound** work: web servers, database queries, HTTP calls — anywhere you spend most of your time *waiting*.

```
One thread: [task A ...await][task B ...await][task A resumes][task C]...
             ↑ task A hit a DB call, so the runtime runs B meanwhile
```

### The one-sentence rule

> **Waiting on the network/disk/DB? → async. Burning CPU? → threads.**

A web server like the one in Zero2Prod handles 10,000 connections that are each mostly *waiting* (for the client, for Postgres). 10,000 OS threads would be wasteful; 10,000 async tasks on 8 threads is cheap. That's why Zero2Prod is built on tokio.

Key mental model shift: **async is not about speed, it's about not wasting threads while waiting.**

---

## 2. Part I — OS Threads

Before async, understand threads. Rust's `std::thread` is simple:

```rust
use std::thread;

fn main() {
    let handle = thread::spawn(|| {
        println!("hello from spawned thread");
        42 // threads can return values
    });

    println!("hello from main thread");

    // .join() blocks until the thread finishes, gives you its return value
    let result = handle.join().unwrap();
    println!("thread returned {result}");
}
```

Three things to internalize:

**1. `spawn` takes a closure that must be `'static`.**
The thread might outlive the function that spawned it, so it can't borrow local variables — it must *own* everything it uses. This is why you see `move`:

```rust
let name = String::from("Ayoub");

// ❌ won't compile: closure borrows `name`, but thread may outlive main
// thread::spawn(|| println!("{name}"));

// ✅ move ownership into the closure
thread::spawn(move || println!("{name}"));
```

**2. Without `join`, the thread may never run.**
When `main` exits, all threads are killed. `join()` waits.

**3. Threads run in arbitrary order.**
Run this twice, get different output orders. The OS decides.

```rust
for i in 0..5 {
    thread::spawn(move || println!("thread {i}"));
}
// Output order is unpredictable. Some may not print at all (main exits first)!
```

### Scoped threads (Rust 1.63+): borrowing IS allowed

`thread::scope` guarantees threads finish before the scope ends, so borrowing works:

```rust
let data = vec![1, 2, 3];

thread::scope(|s| {
    s.spawn(|| println!("first: {}", data[0]));   // borrows, no move needed
    s.spawn(|| println!("len: {}", data.len()));
}); // ← all threads joined here, guaranteed

println!("data still usable: {data:?}");
```

---

## 3. Part II — Sharing Data

The hard part of multithreading: two threads touching the same data. Rust makes data races a **compile error**, which is why it feels harder than other languages — the compiler forces you to pick a strategy.

### Strategy 1: `Arc<T>` — shared *read-only* access

`Rc<T>` is a reference-counted pointer but isn't thread-safe. `Arc<T>` (Atomic Rc) is:

```rust
use std::sync::Arc;
use std::thread;

let data = Arc::new(vec![1, 2, 3, 4, 5]);

let mut handles = vec![];
for i in 0..3 {
    let data = Arc::clone(&data); // cheap: copies the pointer, bumps a counter
    handles.push(thread::spawn(move || {
        println!("thread {i} sees sum = {}", data.iter().sum::<i32>());
    }));
}
for h in handles { h.join().unwrap(); }
```

`Arc` alone gives **shared immutable** access. To mutate, add a lock.

### Strategy 2: `Arc<Mutex<T>>` — shared *mutable* access

```rust
use std::sync::{Arc, Mutex};
use std::thread;

let counter = Arc::new(Mutex::new(0));

let mut handles = vec![];
for _ in 0..10 {
    let counter = Arc::clone(&counter);
    handles.push(thread::spawn(move || {
        let mut num = counter.lock().unwrap(); // blocks until lock acquired
        *num += 1;
    })); // ← lock released here, when `num` (the guard) is dropped
}
for h in handles { h.join().unwrap(); }

println!("count = {}", *counter.lock().unwrap()); // always 10
```

Key points:
- `lock()` returns a `MutexGuard` — a smart pointer to the data. The lock releases when the guard is **dropped** (end of scope). No manual unlock.
- Holding a lock too long serializes your program. Lock, do the minimum, drop.
- `RwLock<T>` is like Mutex but allows many readers OR one writer — good when reads vastly outnumber writes.

### The two magic traits: `Send` and `Sync`

You'll see these in error messages constantly. They're auto-derived marker traits:

- **`Send`**: the type can be *moved* to another thread. Almost everything is `Send`. Notable exception: `Rc<T>`.
- **`Sync`**: the type can be *shared* (`&T`) between threads. Notable exceptions: `RefCell<T>`, `Cell<T>`.

Rules of thumb:
- Error says "`Rc<...> cannot be sent between threads`" → use `Arc`.
- Error says "`RefCell<...> cannot be shared`" → use `Mutex`.
- These same errors appear in async code with `tokio::spawn` — same fix.

---

## 4. Part III — Channels

Alternative philosophy: **don't share memory; send messages.** One thread owns the data, others send it work / receive results.

```rust
use std::sync::mpsc; // "multi-producer, single-consumer"
use std::thread;

let (tx, rx) = mpsc::channel();

// producer thread
thread::spawn(move || {
    for i in 0..5 {
        tx.send(i).unwrap(); // ownership of `i` moves through the channel
    }
}); // tx dropped here → channel closes

// consumer: rx is an iterator that ends when all senders are dropped
for received in rx {
    println!("got {received}");
}
```

Multi-producer: clone the sender.

```rust
let (tx, rx) = mpsc::channel();
for id in 0..3 {
    let tx = tx.clone();
    thread::spawn(move || tx.send(format!("hello from {id}")).unwrap());
}
drop(tx); // drop the original, or rx never sees "channel closed"
for msg in rx { println!("{msg}"); }
```

Channels reappear in tokio (`tokio::sync::mpsc`, `oneshot`, `broadcast`, `watch`) with the same mental model, just async.

---

## 5. Part IV — Futures

Now the async half. This is where most confusion lives, so let's go slow.

### A Future is a value representing "a result that isn't ready yet"

```rust
pub trait Future {
    type Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}

pub enum Poll<T> {
    Ready(T),   // done, here's the value
    Pending,    // not done, try again later
}
```

That's the whole trait. A future is just a struct with a `poll` method that either says "done, here's your value" or "not yet."

### The three facts that dissolve most confusion

**Fact 1: Futures are LAZY. Nothing happens until you `.await` them.**

```rust
async fn say_hello() {
    println!("hello");
}

fn main2() {
    let fut = say_hello(); // ← prints NOTHING. Just builds a value.
    // hello is never printed unless something awaits/polls `fut`
}
```

This is unlike JavaScript, where a Promise starts running the moment you create it. In Rust, an `async fn` call is like writing a to-do note — nobody does the work until an executor polls it.

**Fact 2: `async fn` is sugar for "a function returning a Future".**

```rust
async fn fetch_num() -> u32 { 42 }

// is exactly equivalent to:
fn fetch_num() -> impl Future<Output = u32> {
    async { 42 }
}
```

The compiler transforms the body into a **state machine** — a struct that remembers where it was between `poll` calls. Every `.await` is a possible pause point where the state machine can be suspended and resumed later.

**Fact 3: Someone has to poll. That someone is the runtime (executor).**

`Future` is just a trait — the standard library defines it but provides **no engine to run it**. That's why tokio exists. The executor's job, simplified:

```
loop:
    poll the future
    if Ready(value) → done
    if Pending      → park this task; the future arranged (via the Waker
                      inside Context) to be woken when progress is possible;
                      meanwhile, run other tasks
```

This is why async is efficient: `Pending` doesn't block a thread. The thread immediately moves on to poll a *different* task.

### What about `Pin`?

You'll see `Pin<&mut Self>` and panic. Short version: the state machine may contain references into itself (a borrow across an `.await`), so moving it in memory would break those references. `Pin` is a promise that the future won't be moved once polling starts. **For application code (and all of Zero2Prod) you almost never touch Pin directly.** File it under "I know why it exists" and move on. Come back to it if you ever write a manual `Future` impl.

---

## 6. Part V — async/await

### Syntax basics

```rust
async fn get_user(id: u64) -> String {
    // .await = "pause me here until this future is ready;
    //           run other tasks meanwhile"
    let data = fetch_from_db(id).await;
    format!("user: {data}")
}

async fn fetch_from_db(id: u64) -> String {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await; // fake IO
    format!("row-{id}")
}
```

Rules:
- `.await` only works **inside** `async fn` / `async {}` blocks.
- `main` can't be `async`... unless a runtime macro makes it so (`#[tokio::main]`).
- An `async fn`'s return value is a future; you get the real value by awaiting it.

### Sequential vs concurrent — THE key insight

```rust
// SEQUENTIAL: takes ~200ms. await runs futures one at a time.
let a = fetch_from_db(1).await;
let b = fetch_from_db(2).await;

// CONCURRENT: takes ~100ms. join! polls both at once.
let (a, b) = tokio::join!(fetch_from_db(1), fetch_from_db(2));
```

`.await` in a row = one after another. If you want things to overlap, you must say so, with `join!`, `select!`, or `spawn`. **Writing `async` doesn't magically parallelize anything.**

### async blocks and moving data in

```rust
let name = String::from("Ayoub");
let fut = async move {          // `move` works like closures
    println!("hello {name}");
};
fut.await;
```

### Traps around `.await`

**Don't hold a `std::sync::MutexGuard` across an `.await`:**

```rust
// ❌ BAD: lock held while suspended; other tasks on this thread deadlock,
//         and the compiler may reject it (guard isn't Send).
let guard = mutex.lock().unwrap();
some_async_op().await;
drop(guard);

// ✅ Either release before awaiting:
let value = { mutex.lock().unwrap().clone() };
some_async_op().await;

// ✅ Or use tokio::sync::Mutex, whose lock() is itself async:
let guard = tokio_mutex.lock().await;
some_async_op().await; // OK (but prefer std Mutex + short critical sections)
```

**Don't block inside async:**

```rust
// ❌ BAD: freezes the whole worker thread; other tasks on it starve
std::thread::sleep(Duration::from_secs(1));
let bytes = std::fs::read("big.bin").unwrap(); // blocking IO — same problem

// ✅ async versions
tokio::time::sleep(Duration::from_secs(1)).await;
let bytes = tokio::fs::read("big.bin").await?;

// ✅ CPU-heavy or unavoidably-blocking work → dedicated blocking pool
let hash = tokio::task::spawn_blocking(move || expensive_hash(password)).await?;
```

---

## 7. Part VI — Tokio

Tokio provides: the **executor** (polls your futures), the **reactor** (talks to the OS about network/timer readiness), a **thread pool** (multi-threaded by default), and async versions of IO/sync primitives.

### Setup

```toml
# Cargo.toml
[dependencies]
tokio = { version = "1", features = ["full"] }
```

```rust
#[tokio::main]                    // expands to: build runtime, block_on(async main)
async fn main() {
    println!("running on tokio");
}
```

### `tokio::spawn` — the async equivalent of `thread::spawn`

```rust
#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        "task done"
    });

    // The task runs concurrently starting NOW (spawn is the exception
    // to laziness — the runtime polls it without you awaiting).
    println!("main keeps going");

    let result = handle.await.unwrap(); // JoinHandle is itself a future
    println!("{result}");
}
```

Crucial detail: a spawned task may be moved between worker threads, so its future must be **`Send + 'static`** — same `move`/ownership rules as `thread::spawn`, and the same `Arc`/`Mutex` fixes.

### `join!` vs `spawn` vs `select!`

```rust
use tokio::time::{sleep, Duration};

// join! — run futures concurrently on the CURRENT task, wait for ALL.
// No Send requirement, no allocation. Use for a fixed set of operations.
let (a, b, c) = tokio::join!(
    fetch(1),
    fetch(2),
    fetch(3),
);

// spawn — create an independent task, possibly on another thread.
// Use when: dynamic number of jobs, want true parallelism, or
// fire-and-forget. try_join_all / JoinSet for collections:
let mut set = tokio::task::JoinSet::new();
for id in 0..10 {
    set.spawn(fetch(id));
}
while let Some(res) = set.join_next().await {
    println!("{:?}", res.unwrap());
}

// select! — wait for the FIRST to finish, cancel the rest.
// Classic use: timeouts.
tokio::select! {
    result = fetch(1) => println!("got {result}"),
    _ = sleep(Duration::from_secs(2)) => println!("timed out!"),
}
// (For plain timeouts, tokio::time::timeout(dur, fut) is more direct.)
```

### Cancellation — async's sharp edge

Dropping a future **cancels it**: it simply never gets polled again. `select!` drops the losers. This means an async operation can stop *between any two `.await` points* and never resume. Keep cleanup in `Drop` impls if it must run, and be aware that "the code after `.await`" is not guaranteed to execute. (You mostly notice this once you use `select!` and timeouts.)

### Async channels

Same idea as `std::sync::mpsc`, but `send`/`recv` are async and there are more flavors:

```rust
use tokio::sync::{mpsc, oneshot};

// mpsc: many producers, one consumer — the workhorse
let (tx, mut rx) = mpsc::channel::<String>(32); // bounded: backpressure!

tokio::spawn(async move {
    tx.send("job 1".into()).await.unwrap(); // waits if buffer full
});

while let Some(msg) = rx.recv().await {
    println!("got {msg}");
}

// oneshot: single value, single use — "reply channels"
let (tx, rx) = oneshot::channel::<u32>();
tokio::spawn(async move { let _ = tx.send(42); });
let answer = rx.await.unwrap();

// Also: broadcast (many→many), watch (latest-value, e.g. config updates)
```

### A complete mini TCP echo server (tokio's "hello world")

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("listening on 8080");

    loop {
        let (mut socket, addr) = listener.accept().await?;
        // one lightweight task per connection — this is the whole trick
        tokio::spawn(async move {
            println!("client: {addr}");
            let mut buf = [0u8; 1024];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => return,                       // client closed
                    Ok(n) => {
                        if socket.write_all(&buf[..n]).await.is_err() { return; }
                    }
                    Err(_) => return,
                }
            }
        });
    }
}
```

Test with `nc 127.0.0.1 8080` in another terminal. Open several — they're all served concurrently, probably on a handful of threads. This structure — `accept` loop + `spawn` per connection — is what actix-web/axum do for you under the hood.

### Streams — async iterators (brief)

```rust
use tokio_stream::{self as stream, StreamExt}; // crate: tokio-stream

let mut s = stream::iter(vec![1, 2, 3]);
while let Some(x) = s.next().await {   // like Iterator, but next() is async
    println!("{x}");
}
```

You'll meet these as "a sequence of incoming things": websocket messages, SQL result rows, file lines. `while let Some(x) = s.next().await` is the pattern.

---

## 8. Part VII — Pitfalls

The greatest hits of async confusion, collected:

| # | Symptom | Cause | Fix |
|---|---------|-------|-----|
| 1 | "nothing happens" | created a future, never awaited it (compiler warns: *unused future*) | `.await` it, or `tokio::spawn` it |
| 2 | everything is slow / sequential | `.await`-ing in a loop, one at a time | `join!`, `JoinSet`, or `futures::future::join_all` |
| 3 | server freezes under load | blocking call (`std::thread::sleep`, sync DB driver, heavy CPU) inside async | `tokio::time::sleep`, async drivers, or `spawn_blocking` |
| 4 | "future cannot be sent between threads safely" | non-`Send` value (e.g. `Rc`, `RefCell`, `std MutexGuard`) held **across** an `.await` in a spawned task | drop it before the `.await`; use `Arc`/`Mutex`; narrow the scope with `{ }` |
| 5 | "borrowed value does not live long enough" on spawn | task must be `'static` but borrows a local | `move` + clone/`Arc` what you need |
| 6 | deadlock | std `MutexGuard` held across `.await` | release before await, or `tokio::sync::Mutex` |
| 7 | "`await` is only allowed inside `async` functions" | awaiting in a sync fn | make it async, or bridge with `runtime.block_on(fut)` (never call `block_on` *inside* async code) |
| 8 | work vanishes mid-flight | future was dropped (task cancelled, `select!` loser, client disconnected) | this is cancellation; design for it |
| 9 | "cannot start a runtime from within a runtime" | `#[tokio::main]` code calling something that builds its own runtime / `block_on` | you're already async; just `.await` |

One more conceptual one: **concurrency ≠ parallelism.** `join!` on a single-threaded runtime is concurrent (interleaved) but not parallel (never simultaneous). `tokio::spawn` on the default multi-thread runtime can be truly parallel. For IO-bound work the distinction rarely matters; for CPU-bound work it does.

---

## 9. Part VIII — Zero2Prod

Zero2Prod uses actix-web + tokio + sqlx. Here's how everything above maps:

```rust
// This is Zero2Prod's skeleton. Annotated with what you now know:

#[tokio::main]                       // ← builds the tokio runtime (Part VI)
async fn main() -> std::io::Result<()> {
    let pool = PgPoolOptions::new()  // sqlx connection pool = shared state
        .connect(&url).await?;       // ← async IO: doesn't block a thread

    HttpServer::new(move || {
        App::new()
            .route("/health_check", web::get().to(health_check))
            .app_data(web::Data::new(pool.clone()))
            //        ^^^^^^^^^ web::Data is an Arc in disguise! (Part II)
    })
    .listen(listener)?
    .run()                            // accept-loop + task-per-connection,
    .await                            // like our echo server (Part VI)
}

async fn health_check() -> HttpResponse {   // every handler is an async fn;
    HttpResponse::Ok().finish()             // actix spawns/polls it for you
}

async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,          // ← the Arc-like shared pool
) -> HttpResponse {
    sqlx::query!("INSERT INTO subscriptions ...")
        .execute(pool.get_ref())
        .await                        // ← while Postgres works, this worker
                                      //   thread serves OTHER requests
        .map(|_| HttpResponse::Ok().finish())
        .unwrap_or_else(|_| HttpResponse::InternalServerError().finish())
}
```

Decoder ring for the book:

| You'll see in Zero2Prod | It is |
|---|---|
| `#[tokio::main]` / `#[actix_web::main]` | runtime setup; actix's macro wraps tokio |
| handlers are `async fn` | each request = a future, polled by tokio workers |
| `web::Data<T>` | `Arc<T>` shared across all workers/requests |
| `.await` on sqlx queries | async IO — the whole reason the server scales |
| `tokio::spawn` in tests | run the app in the background while the test makes HTTP requests to it |
| `PgPool` cloned freely | it's internally an Arc; clones share the pool |
| `Send + 'static` bound errors | Part VII, rows 4–5 |

If you're comfortable with Parts V–VII, nothing async in Zero2Prod should surprise you anymore — the book adds *web* concepts (routing, extractors, migrations, tracing), not new *async* concepts.

---

## 10. Exercises

Do these in order. Each builds on the last. Create a fresh project per exercise (`cargo new ex01_threads`) or one project with `src/bin/ex01.rs`, `ex02.rs`, ... (run with `cargo run --bin ex01`). Exercises 4+ need tokio in Cargo.toml.

### Ex 1 — Threads warm-up (std only)
Spawn 4 threads. Each computes the sum of one quarter of the numbers 1..=1000 and *returns* it via `join()`. Main adds the four partial sums and asserts the total is 500500.
*Practices: `thread::spawn`, `move`, `join` with return values.*

### Ex 2 — Shared counter, two ways (std only)
10 threads each increment a shared counter 1000 times.
- (a) with `Arc<Mutex<u64>>`
- (b) with `Arc<AtomicU64>` (`fetch_add`, `Ordering::Relaxed`)
Assert both end at 10_000.
*Practices: Arc, Mutex, lock scope, atomics.*

### Ex 3 — Worker pool with channels (std only)
One `mpsc` channel of jobs (`u64` values), 3 worker threads. Each worker receives numbers and prints `worker {id}: fib({n}) = {result}` using a naive recursive fibonacci. Main sends 40, 41, 42, 35, 30 then drops the sender; workers exit when the channel closes.
Hint: `std::sync::mpsc::Receiver` can't be cloned — wrap it in `Arc<Mutex<Receiver>>` and let workers `lock().unwrap().recv()`.
*Practices: channels, ownership through channels, shared receiver.*

### Ex 4 — First tokio program
`#[tokio::main]`. Write `async fn slow(name: &str, ms: u64) -> String` that sleeps `ms` milliseconds (tokio sleep!) then returns `format!("{name} done")`.
- (a) Await `slow("A", 300)` then `slow("B", 300)` sequentially — time it with `std::time::Instant`, expect ~600ms.
- (b) Run both with `tokio::join!` — expect ~300ms.
Print both timings. *Feel* the difference — this is the core insight.
*Practices: runtime setup, laziness, join! vs sequential await.*

### Ex 5 — Spawn and gather
Spawn 5 tasks with `tokio::spawn`; task `i` sleeps `100 * i` ms then returns `i * i`. Collect all `JoinHandle`s in a `Vec`, await them all, sum the results (should be 30). Then redo it with `JoinSet` — notice results arrive in *completion* order, not spawn order.
*Practices: spawn, JoinHandle, Send + 'static, JoinSet.*

### Ex 6 — Timeout with select!
Write `async fn flaky() -> u32` that sleeps a random-ish duration (e.g. seed from `std::time::SystemTime` nanos % 2000 ms) then returns 7. Race it against a 1-second timeout using `tokio::select!`. Print `"got 7"` or `"too slow"`. Run several times. Then rewrite using `tokio::time::timeout` and note it returns `Result`.
*Practices: select!, cancellation, timeout.*

### Ex 7 — Async worker pool (Ex 3, reborn)
Redo exercise 3 with tokio: `tokio::sync::mpsc::channel(8)`, 3 spawned worker tasks. Trap included on purpose: naive `fib(42)` is CPU-bound and will freeze other tasks. First do it wrong and observe (add a 4th task printing a heartbeat every 100ms — watch it stall). Then fix with `spawn_blocking` and watch the heartbeat stay smooth.
*Practices: async channels, the blocking trap, spawn_blocking. This one teaches the most.*

### Ex 8 — Mini chat server (capstone)
Extend the echo server from Part VI into a broadcast chat: every line a client sends is forwarded to all other connected clients. Use `tokio::sync::broadcast::channel(16)`. Per connection: `select!` between "read a line from this socket" and "receive a broadcast message". Test with 2–3 `nc 127.0.0.1 8080` terminals.
Stretch: first line a client sends is their name; prefix messages with it.
*Practices: everything — spawn per connection, broadcast channel, select! in a loop, split socket read/write halves (`socket.split()` or `into_split()`).*

### Ex 9 — Bridge to Zero2Prod
Write a "server" without any framework: tokio `TcpListener`, and for every connection, read the request bytes, and if the first line starts with `GET /health_check`, reply with the raw bytes `"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"`, else reply `404 Not Found` similarly. Check it with `curl -v localhost:8080/health_check`. Now open Zero2Prod chapter 3 and appreciate what actix-web abstracts away.
*Practices: everything again, plus demystifying HTTP.*

---

## 11. Solutions

Hints first — try each exercise for 20+ minutes before peeking. Full solutions for the trickiest ones only; the point is fighting the borrow checker yourself.

<details>
<summary><strong>Ex 1 hint</strong></summary>

```rust
let handles: Vec<_> = (0..4).map(|i| {
    thread::spawn(move || {
        let start = i * 250 + 1;
        let end = (i + 1) * 250;
        (start..=end).sum::<u64>()
    })
}).collect();
let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
```
</details>

<details>
<summary><strong>Ex 2 hint</strong></summary>

Mutex version is Part II verbatim. Atomic version:
```rust
use std::sync::atomic::{AtomicU64, Ordering};
let counter = Arc::new(AtomicU64::new(0));
// in each thread:
counter.fetch_add(1, Ordering::Relaxed);
```
No lock needed — the hardware does the synchronization.
</details>

<details>
<summary><strong>Ex 3 solution (the shared-receiver trick is non-obvious)</strong></summary>

```rust
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

fn fib(n: u64) -> u64 { if n < 2 { n } else { fib(n-1) + fib(n-2) } }

fn main() {
    let (tx, rx) = mpsc::channel::<u64>();
    let rx = Arc::new(Mutex::new(rx));

    let handles: Vec<_> = (0..3).map(|id| {
        let rx = Arc::clone(&rx);
        thread::spawn(move || loop {
            // lock ONLY to receive, release before computing
            let job = rx.lock().unwrap().recv();
            match job {
                Ok(n) => println!("worker {id}: fib({n}) = {}", fib(n)),
                Err(_) => break, // channel closed
            }
        })
    }).collect();

    for n in [40, 41, 42, 35, 30] { tx.send(n).unwrap(); }
    drop(tx);
    for h in handles { h.join().unwrap(); }
}
```
Note the guard is dropped immediately (`rx.lock().unwrap().recv()` is one expression) so workers don't hold the lock while computing fib.
</details>

<details>
<summary><strong>Ex 4 hint</strong></summary>

```rust
let t = std::time::Instant::now();
let (a, b) = tokio::join!(slow("A", 300), slow("B", 300));
println!("{a}, {b} in {:?}", t.elapsed());
```
If your "concurrent" version still takes 600ms, you awaited the futures before passing them to join! — pass the un-awaited futures.
</details>

<details>
<summary><strong>Ex 5 hint</strong></summary>

```rust
let handles: Vec<_> = (0..5u64).map(|i| tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(100 * i)).await;
    i * i
})).collect();
let mut sum = 0;
for h in handles { sum += h.await.unwrap(); }
```
</details>

<details>
<summary><strong>Ex 6 hint</strong></summary>

```rust
tokio::select! {
    v = flaky() => println!("got {v}"),
    _ = tokio::time::sleep(Duration::from_secs(1)) => println!("too slow"),
}
```
</details>

<details>
<summary><strong>Ex 7 solution (the whole point of the exercise)</strong></summary>

```rust
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

fn fib(n: u64) -> u64 { if n < 2 { n } else { fib(n-1) + fib(n-2) } }

#[tokio::main(flavor = "current_thread")] // single thread makes the freeze obvious
async fn main() {
    let (tx, rx) = mpsc::channel::<u64>(8);
    let rx = std::sync::Arc::new(tokio::sync::Mutex::new(rx));

    // heartbeat — the canary
    tokio::spawn(async {
        loop {
            sleep(Duration::from_millis(100)).await;
            println!("  ...heartbeat");
        }
    });

    let workers: Vec<_> = (0..3).map(|id| {
        let rx = rx.clone();
        tokio::spawn(async move {
            loop {
                let job = rx.lock().await.recv().await;
                let Some(n) = job else { break };

                // ❌ WRONG (try this first): freezes the heartbeat
                // let r = fib(n);

                // ✅ RIGHT: CPU work off the async threads
                let r = tokio::task::spawn_blocking(move || fib(n)).await.unwrap();

                println!("worker {id}: fib({n}) = {r}");
            }
        })
    }).collect();

    for n in [40u64, 41, 42, 35, 30] { tx.send(n).await.unwrap(); }
    drop(tx);
    for w in workers { w.await.unwrap(); }
}
```
With the wrong version the heartbeat stops for seconds at a time; with `spawn_blocking` it ticks steadily. That contrast is the lesson.
</details>

<details>
<summary><strong>Ex 8 skeleton</strong></summary>

```rust
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    let (tx, _) = broadcast::channel::<(String, std::net::SocketAddr)>(16);

    loop {
        let (socket, addr) = listener.accept().await?;
        let tx = tx.clone();
        let mut rx = tx.subscribe();

        tokio::spawn(async move {
            let (reader, mut writer) = socket.into_split();
            let mut lines = BufReader::new(reader).lines();
            loop {
                tokio::select! {
                    line = lines.next_line() => {
                        match line {
                            Ok(Some(msg)) => { let _ = tx.send((msg, addr)); }
                            _ => break, // disconnected
                        }
                    }
                    msg = rx.recv() => {
                        if let Ok((msg, sender)) = msg {
                            if sender != addr {   // don't echo to self
                                let _ = writer.write_all(format!("{sender}: {msg}\n").as_bytes()).await;
                            }
                        }
                    }
                }
            }
        });
    }
}
```
</details>

<details>
<summary><strong>Ex 9 hint</strong></summary>

Read once into a buffer, `String::from_utf8_lossy`, check `.starts_with("GET /health_check")`. Don't try to parse HTTP properly — the exercise is seeing that HTTP is just bytes over the TCP socket you already know.
</details>

---

## 12. How to Read the Flitton *Async Rust* Book

*Async Rust: Unleashing the Power of Fearless Concurrency* — Maxwell Flitton & Caroline Morton (O'Reilly, 2024). Chapters:

| Ch | Title | What it covers |
|----|-------|----------------|
| 1 | Introduction to Async | processes vs threads vs async, file IO, HTTP |
| 2 | Basic Async Rust | futures, pinning, wakers, contexts, sharing data |
| 3 | Building Our Own Async Queues | write your own runtime, task stealing |
| 4 | Integrating Networking into Our Own Async Runtime | hyper, AsyncRead/AsyncWrite by hand |
| 5 | Coroutines | how coroutines relate to async |
| 6 | Reactive Programming | pub/sub patterns |
| 7 | Customizing Tokio | runtime configuration internals |
| 8 | The Actor Model | actors built from channels + tasks |
| 9 | Design Patterns | retry, circuit breaker, state machine, decorator, waterfall |
| 10 | Async TCP Server with std only | networking without tokio |
| 11 | Testing | unit testing async code |

### Why the book felt confusing

It's a **bottom-up internals book**. Chapters 3–5 have you build your own runtime, executor, wakers, and coroutines — deep-end material that answers "how does the engine work?" But when you're starting out (and reading Zero2Prod), your question is "how do I drive?" It's not the wrong book — it's the wrong *order* to read it in.

### Read it in two passes

**Pass 1 — now, alongside Zero2Prod:**

- **Ch 1** — read fully. Matches Part 1 of this guide.
- **Ch 2** — read carefully, but *skim* the "Pinning" and "Waking Futures Remotely" sections (know they exist; don't try to master them). "Sharing Data Between Futures" maps to Parts 2–3 here; the rest maps to Parts 4–5. Pair it with Exercises 4–6.
- **Ch 3–7 — skip entirely.** Sticky note: "later."
- **Ch 8 (Actors)** — read. Practical pattern built from channels + spawned tasks; a natural extension of Exercise 7.
- **Ch 9 (Design Patterns)** — read. Retry and circuit-breaker come up directly in Zero2Prod's HTTP-client chapters.
- **Ch 11 (Testing)** — read when Zero2Prod reaches integration testing (its chapters 3–4).

**Pass 2 — after finishing Zero2Prod (or once tokio feels comfortable):**

- Ch 3 → 4 → 5 → 7 → 10, in order. These answer "what does `.await` *actually* do?" by making you build the machinery yourself. Rewarding once you've used async in a real project; bewildering before.

### How to read (matters as much as the order)

1. **Type every code block by hand** into a `cargo new` project. No copy-paste. Fingers learn what eyes skip.
2. **Run it, then break it.** Remove an `.await`, remove a `move`, read the compiler error. Errors teach faster than prose.
3. **One chapter per sitting, max.** Async needs digestion time.
4. **Confused paragraph → flag it and keep going.** Don't reread it five times; return after the next chapter. The books often explain forward-references later.
5. **Don't resource-hop.** Stacking three books at once with no code written is exactly how confusion happens.

Rule of thumb:

> **Flitton's book = how async works underneath. This guide + tokio tutorial = how to use it. Zero2Prod = how to ship with it.** You need "use" before "underneath."

---

## 13. The 7-Day Study Plan

If async still feels like fog, stop reading and start typing. ~1 hour per day:

### Day 1–2 — Threads first (no tokio)
- Read Parts 1–3 of this guide.
- Do Exercises 1, 2, 3.
- Threads are concrete — real OS objects, no magic. They're the foundation for the async mental model.

### Day 3 — The "aha" day
- Read Parts 4–5 only.
- Do Exercise 4. Watch 600ms become 300ms with `join!`. That moment is async understood.
- Then reread this sentence until it clicks: *an `async fn` does nothing until awaited; `.await` means "pause me here, run other tasks."*

### Day 4–5 — Tokio proper
- Read Part 6. Do Exercises 5, 6, 7.
- Exercise 7 is mandatory: freeze the runtime on purpose, then fix it with `spawn_blocking`. It teaches what tokio *is* better than any chapter.

### Day 6 — Capstone
- Exercise 8, the chat server. It's hard. Expect to struggle for 1–2 hours. The struggle is the learning.

### Day 7 — Back to Zero2Prod
- Read Part 9 (the decoder ring), then reopen Zero2Prod chapters 1–3. They will read completely differently.

### Rules for the week

1. **Flitton book stays closed** until Day 7 at the earliest (then follow the Pass-1 plan above).
2. **Compiler error you don't understand → stop and decode it.** The error text almost always names the concept (Send, 'static, borrow across await). Look it up in Part VII's table.
3. **Ask small questions.** "I don't get async" is too big to answer; "why does `join!` need un-awaited futures?" is answerable.
4. **No new resources this week.** One guide, nine exercises. Done.

Understanding async isn't reading harder — it's your fingers hitting the `Send + 'static` error, fixing it, hitting it again, and fixing it faster. Three hours of typed code beats thirty hours of reading.

---

## Further reading (in order of usefulness for you)

1. **Tokio tutorial** — https://tokio.rs/tokio/tutorial — the best practical async resource; builds a mini-Redis.
2. **Async Book** — https://rust-lang.github.io/async-book/ — the theory (Futures, Pin, executors). Read chapters 1–4, skip the rest for now.
3. **Rust Book ch. 16–17** — threads, channels, Arc/Mutex, and the new async chapters.
4. **Alice Ryhl — "Async: What is blocking?"** — https://ryhl.io/blog/async-what-is-blocking/ — short, explains pitfall #3 better than anything else.
5. **Jon Gjengset's "Crust of Rust: async/await"** (YouTube) — when you want to go deep.

Good luck — do the exercises, especially 4 and 7. Reading about async teaches ~20% of it; fighting the compiler teaches the rest.

# Learn Async Rust

Companion code for [roadmap.md](roadmap.md) — a practical guide to OS threads, async/await, and tokio, written for people who already know Rust syntax but are new to concurrency.

**Goal:** build enough intuition to read [*Zero To Production In Rust*](https://www.zero2prod.com/) (or Maxwell Flitton's *Async Rust*) without getting lost. If you're about to start either book and concurrency still feels like fog, start here first.

## How this repo is organized

Every part of the guide has a matching, runnable chapter module:

```
src/
  ch1/ch1.rs   <- Part I: OS Threads (std::thread)
  lib.rs       <- declares/re-exports every chX module
  main.rs      <- scratch harness; calls chapter functions to try them out
```

The convention: each chapter lives in a folder `chX/` containing `chX.rs`, declared as a module in `lib.rs`. New chapters slot in the same way as the guide grows.

`main.rs` is just a personal test harness, not a guided entry point — read the source tree directly to explore a chapter's code.

## How to follow along

1. Read the matching section of [roadmap.md](roadmap.md) — it has the explanation, the mental model, and annotated code.
2. Open the matching `src/chX/chX.rs` and read the runnable version of the same ideas.
3. Run it with `cargo run` — edit `main.rs` to call whichever function you want to poke at.
4. Do the exercises in roadmap.md §10 (solutions/hints in §11). This is where the concepts actually stick — reading teaches a fraction of what fighting the compiler does.

## Where to start

- **New to concurrency entirely?** Follow the [7-Day Study Plan](roadmap.md#13-the-7-day-study-plan) — it sequences the parts, the exercises, and even when to pick up Flitton's book.
- **Already comfortable with threads?** Jump straight to [Part IV — Futures](roadmap.md#5-part-iv--futures).
- **About to start Zero2Prod?** Read at least through [Part VIII](roadmap.md#9-part-viii--zero2prod) — it's a decoder ring mapping this guide's concepts onto Zero2Prod's actual code.

## Requirements

- Rust, edition 2024
- `cargo run` to execute. Chapters on futures/tokio (Part IV onward) will need `tokio` added to `Cargo.toml` — add it when the guide tells you to.

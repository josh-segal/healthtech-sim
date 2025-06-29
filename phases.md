Here’s a **task-by-task implementation plan** optimized for the `actor-ready` structure you shared. The order builds incrementally—from parsing and core actors to message flow, timing, and monitoring.

---

## 🟢 **Phase 1: Foundations — Types, Input, and Setup**

### 1. **`schema.rs` — Define `PayerClaim`**

* Implement the struct to match the provided JSON schema.
* Use `serde::{Deserialize}` for line-by-line parsing.

> ✅ Test: parse a sample claim from stdin or file.

---

### 2. **`config.rs` — CLI Argument Parsing**

* Use `clap` or `std::env::args()` to read the input file path and optional config (e.g. ingest rate).

> ✅ Test: run the app and see it read config values.

---

### 3. **`messages.rs` — Define Message Enums**

* Create:

  * `ClaimMessage`: e.g. `NewClaim(PayerClaim)`
  * `PayerMessage`: e.g. `Adjudicate(PayerClaim)`
  * `RemittanceMessage`: e.g. `Processed(Remittance)`

> 🧠 Tip: Derive `Debug` to simplify logging.

---

## 🧵 **Phase 2: Core Actors (Biller → Clearinghouse → Payer)**

### 4. **`biller.rs` — Async Claim Reader**

* Ingest claims from file at a configurable rate.
* Send them through an `mpsc::Sender<ClaimMessage>`.

> ✅ Test: confirm messages sent and print from channel.

---

### 5. **`clearinghouse.rs` — Route Claims to Payer**

* Receive `ClaimMessage` from biller.
* Extract payer ID, forward to correct `PayerMessage` channel.
* Store timestamp in `HashMap<claim_id, Instant>` for AR aging.

> 🧠 Tip: Consider `tokio::select!` for future shutdown handling.

---

### 6. **`payer.rs` — Adjudicate and Respond**

* Receive `PayerMessage::Adjudicate(claim)`.
* Sleep for random time (bounded by payer's config).
* Generate a `Remittance` and send it back to clearinghouse.

> ✅ Use `rand` + `tokio::time::sleep`.

---

### 7. **`remittance.rs` — Define Remittance Struct**

* Contains `payer_paid_amount`, `copay_amount`, etc.
* Ensure per-line totals match billed amount.

> 🧠 Add logic helpers (e.g., `fn from_claim(...) -> Remittance`).

---

### 8. **Back in `clearinghouse.rs`: Forward Remittance**

* Route `Remittance` back to original `biller`/reporting system.
* Store timestamp for AR age computation.

---

## 📊 **Phase 3: Stats, Monitoring, and Output**

### 9. **`reporter.rs` — AR Aging & Patient Summary**

* Run as a loop with `tokio::time::interval`.
* Print:

  * AR aging buckets (0–1 min, 1–2 min…)
  * Summary stats per patient (copay, deductible, etc.)

> ✅ Pull remittance history from a shared store (`Arc<Mutex<HashMap>>`).

---

### 10. **`utils.rs` — Logging, Time Helpers**

* Add:

  * `elapsed_minutes(start: Instant)`
  * Simple logger wrappers (if needed)

---

## 🚀 **Phase 4: Integration and Orchestration**

### 11. **`main.rs` — Wire Everything Together**

* Initialize:

  * Config
  * Channels (between actors)
  * All actor tasks via `tokio::spawn`

* Use `tokio::try_join!` or let tasks run forever.

> ✅ Run the full simulation pipeline!

---

### 12. **(Optional) Graceful Shutdown**

* Add a shutdown signal (e.g. Ctrl+C handling with `tokio::signal`)
* Use `select!` in each task to terminate cleanly.

---

### 🧪 **Bonus: Tests**

* Add unit tests for:

  * Remittance math (`remittance.rs`)
  * Claim ingestion/parsing (`schema.rs`)
* Add integration test: feed 2–3 claims and assert summary output

---

## 📦 Summary Roadmap

| Order | File               | Focus                  | Purpose                              |
| ----- | ------------------ | ---------------------- | ------------------------------------ |
| 1     | `schema.rs`        | PayerClaim structs     | Parse JSON                           |
| 2     | `config.rs`        | CLI args               | Input file, config rate              |
| 3     | `messages.rs`      | Message enums          | Actor communication                  |
| 4     | `biller.rs`        | Async claim reader     | Send to clearinghouse                |
| 5     | `clearinghouse.rs` | Claim routing logic    | Payer mapping, remittance handling   |
| 6     | `payer.rs`         | Simulated adjudication | Generate remittance w/ delay         |
| 7     | `remittance.rs`    | Response logic         | Total calculation, per-line summary  |
| 8     | `reporter.rs`      | Periodic stats         | AR aging and patient-level summaries |
| 9     | `main.rs`          | Wire up components     | Launch tasks, link channels          |
| 10    | `utils.rs`         | Helpers                | Logging, time, formatting            |
| 11    | (optional) Tests   | Verification           | Logic + integration checks           |



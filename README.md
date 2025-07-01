# HealthTechSim

A high-performance healthcare claim processing simulation built in Rust, demonstrating asynchronous message-passing architecture for medical billing workflows.

## 1. Architecture Overview

HealthTechSim implements a distributed, event-driven architecture that mirrors real-world healthcare claim processing systems. The system uses Rust's async/await with Tokio for high-concurrency processing and channel-based communication for loose coupling between components.

## 2. System Design

```
┌─────────┐    ┌──────────┐    ┌─────────────────┐    ┌─────────┐    ┌─────────────┐
│ Reader  │───▶│  Biller  │───▶│  Clearinghouse  │───▶│  Payer  │───▶│ Remittance  │
│         │    │          │    │                 │    │         │    │             │
│ JSONL   │    │ Throttle │    │ Route & Track   │    │ Adjud.  │    │ Validation  │
│ Input   │    │ Claims   │◀───│ Claims          │◀───│ Claims  │◀───│ & Response  │
└─────────┘    └──────────┘    └─────────────────┘    └─────────┘    └─────────────┘
                                        │                    
                                        │                    
                                        │
                                        │
                                 ┌─────────────┐
                                 │  Reporter   │
                                 │             │
                                 │ Statistics  │
                                 └─────────────┘
```

## 3. Core Components

**Reader** (`src/reader.rs`): An async task that reads healthcare claims from a JSONL file and streams them one by one to the biller. Handles file parsing errors gracefully and logs ingestion progress.

**Biller** (`src/biller.rs`): A rate-limited processor that receives claims from the reader and forwards them to the clearinghouse. Controls the pace of claim processing and manages response channels for each claim to receive remittances.

**Clearinghouse** (`src/clearinghouse.rs`): The central routing hub that directs claims to the appropriate insurance payers based on the payer ID. Tracks claim status throughout processing and routes remittance responses back to the originating biller.

**Payer** (`src/payer.rs`): Simulates an insurance company that adjudicates claims with realistic processing delays. Generates payment responses with detailed breakdowns of what the payer will cover versus patient responsibility.

**Reporter** (`src/reporter.rs`): Monitors the overall system performance by collecting statistics on claim processing times, success rates, and aging analysis from the shared claim history.



// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2026 The Photometoria contributors

mod pool;
pub mod processor;
pub mod queue;
mod worker;

pub use pool::WorkerPool;
pub use processor::{PhotoProcessor, ProcessingResult};
pub use queue::{PhotoBuffer, QueuedPhoto};

# Photo Selection Strategy (Work in Progress)

When implementing the worker pool, a key architectural decision is how workers select the next photo to process. This is especially important when multiple jobs use different AI models.

## The Challenge: Efficiency vs. Fairness

With multiple jobs using different models and limited GPUs, there's a fundamental trade-off:

**Sequential job execution:**
- ✅ Minimal model swaps (one load per job)
- ✅ Maximum efficiency (~6% overhead)
- ❌ Poor fairness (jobs wait in queue until others complete)

**Round-robin photo selection:**
- ✅ Perfect fairness (all jobs progress simultaneously)
- ❌ Excessive model swaps (one per photo if models differ)
- ❌ Severe overhead (~77% in worst case)

## Smart Priority-Based Selection with Hybrid Threshold

A hybrid strategy that balances efficiency and fairness using both count and time constraints:

**Algorithm:**

1. Worker tracks current loaded model, photo counter, and elapsed time since model load
2. When selecting next photo:
   - If `counter < min_photos` OR `elapsed_time < max_time`: Prioritize photos requiring the current model
   - If `counter >= min_photos` AND `elapsed_time >= max_time`: Accept any photo (allow model swap)
3. Reset counter and timer when model changes

**Why Hybrid (Count + Time)?**

- **Count threshold alone**: Unfair when photos have different complexities (large vs small photos)
- **Time threshold alone**: Might swap too early with very fast photos (insufficient amortization of model load overhead)
- **Hybrid**: Guarantees both minimum amortization (count) and temporal fairness (time)

**Pseudo-code:**

```rust
fn select_next_photo(&mut self, min_photos: usize, max_time: Duration) -> Option<Photo> {
    let current_model = self.loaded_model;
    let counter = self.photos_processed;
    let elapsed = self.model_load_time.elapsed();

    // Below thresholds: prefer same model (avoid swap)
    if counter < min_photos || elapsed < max_time {
        if let Some(photo) = find_photo_with_model(current_model) {
            return Some(photo);
        }
    }

    // Above both thresholds: accept any photo (allow model swap for fairness)
    find_any_available_photo()
}
```

## Performance Analysis

**Scenario:** 1 GPU, 2 jobs (50 photos each), different models (qwen3.5, llava)

| Strategy | Time | Overhead | First Result Job B | Fairness |
|----------|------|----------|-------------------|----------|
| Sequential (threshold=∞) | 320s | 20s (6%) | t=170s | Poor |
| Smart (threshold=25) | 340s | 40s (12%) | t=95s | Good |
| Smart (threshold=10) | 420s | 100s (24%) | t=50s | Excellent |
| Round-robin (threshold=1) | 1300s | 1000s (77%) | t=23s | Perfect |

**Key Insights:**

- **Threshold=20-25 photos**: Best balance for production use (~6-12% overhead, good fairness)
- **Threshold=50+ photos**: Equivalent to sequential job execution (maximum efficiency, poor fairness)
- **Threshold=1-5 photos**: Near round-robin behavior (poor efficiency, maximum fairness)

*Note: The analysis above uses count-based thresholds for simplicity. The hybrid approach (count + time) provides superior fairness as explained below.*

## Time-based vs Count-based vs Hybrid Threshold

**The Problem with Count-only Threshold:**

When photos have different processing complexities, count-based thresholds lead to temporal unfairness:

```
Scenario: 1 GPU, 2 jobs, count threshold = 20 photos

Job A: 50 high-res photos (5s each)
Job B: 50 low-res photos (1s each)

Cycle 1:
  Job A: 20 photos × 5s = 100s GPU time
  Job B: 20 photos × 1s = 20s GPU time

Cycle 2:
  Job A: 20 photos × 5s = 100s GPU time
  Job B: 20 photos × 1s = 20s GPU time

Result:
  ❌ Job A gets 5× more GPU time
  ❌ Temporal unfairness
```

**Time-only Threshold:**

Provides temporal fairness but might swap too early:

```
Time threshold = 60s

Job with very fast photos (0.5s each):
  - Processes 120 photos in 60s
  - Model load overhead (10s) well amortized ✓

Job with ultra-fast photos (0.1s each):
  - Processes 600 photos in 60s
  - But could process 100 photos (10s) then swap
  - Model load overhead not fully amortized ⚠️
```

**Hybrid Threshold (Recommended):**

Combines both constraints for optimal behavior:

```
min_photos = 10, max_time = 120s

Job A (high-res, 5s/photo):
  - Processes 10 photos (50s) → min_photos ✓, time < 120s → continues
  - Processes 14 more photos (70s) → 24 total, 120s reached → swaps
  - Result: 24 photos, 120s GPU time

Job B (low-res, 1s/photo):
  - Processes 10 photos (10s) → min_photos ✓, time < 120s → continues
  - Processes 110 more photos (110s) → 120 total, 120s reached → swaps
  - Result: 120 photos, 120s GPU time

✅ Temporal fairness (both get 120s)
✅ Model load overhead well amortized (min 10 photos)
✅ Adapts automatically to photo complexity
```

**Comparison:**

| Threshold Type | Temporal Fairness | Overhead Protection | Complexity |
|----------------|-------------------|---------------------|------------|
| Count-only | ❌ Poor (varies with photo complexity) | ✅ Good | Low |
| Time-only | ✅ Excellent | ⚠️ Moderate (might swap early) | Medium |
| **Hybrid** | **✅ Excellent** | **✅ Excellent** | **Medium** |

## Advantages of Hybrid Approach

**1. Temporal Fairness:**
- Each job receives approximately equal GPU time, regardless of photo complexity
- Prevents jobs with complex photos from dominating GPU resources
- Predictable: "Every job progresses every 2 minutes" (instead of "every N photos")
- Better for multi-user scenarios and QoS/SLA guarantees

**2. Overhead Protection:**
- `min_photos` ensures model load overhead is well amortized
- Won't swap after just 1-2 photos even if time threshold is low
- Protects against pathological cases (ultra-fast tiny photos)

**3. Configurable Trade-off:**
- Tune both dimensions based on workload characteristics
- High thresholds for efficiency-critical workloads
- Low thresholds for user-facing interactive scenarios
- Example: `min_photos=10, max_time=60s` for responsive UI, `min_photos=50, max_time=300s` for batch processing

**4. Adaptive Behavior:**
- If all jobs use same model: zero overhead (never swaps)
- If one job finishes early: continues with remaining job without unnecessary swaps
- Automatically optimal for homogeneous workloads
- Adapts to photo complexity without manual tuning

**5. Better User Experience:**

```
Sequential execution:
  Job A: ████████████████ (completes, then Job B starts)
  Job B: ................ ████████████████

Smart hybrid priority (min=10, max=120s):
  Job A: ███...███...███...███...███
  Job B: ...███...███...███...███...███

Both jobs show progress simultaneously!
Temporal fairness: each gets ~120s per cycle
```

**6. Model Locality:**
- Exploits Ollama's keep-alive feature (models stay in VRAM for 5 minutes by default)
- Processes multiple photos with same model before swapping
- Minimizes expensive model load operations (10-20s per load)

## Implementation Considerations

**Queue Structure:**

```rust
struct PhotoQueue {
    // Photos organized by required model
    photos_by_model: HashMap<ModelId, VecDeque<PhotoId>>,

    // All pending photos (for threshold overflow)
    all_photos: VecDeque<PhotoId>,
}

struct Worker {
    // Current model state
    current_model: ModelId,
    photos_processed: usize,
    model_load_time: Instant,

    // Configuration
    min_photos_before_swap: usize,
    max_time_before_swap: Duration,
}

impl Worker {
    fn should_allow_model_swap(&self) -> bool {
        self.photos_processed >= self.min_photos_before_swap
            && self.model_load_time.elapsed() >= self.max_time_before_swap
    }
}
```

**Configuration:**

```toml
[worker_pool]
# Minimum photos to process before allowing model swap (overhead protection)
min_photos_before_swap = 10

# Maximum time with same model before forcing swap (temporal fairness)
# Format: duration string (e.g., "60s", "2m", "120s")
max_time_before_swap = "120s"
```

**Recommended Values:**

| Use Case | min_photos | max_time | Rationale |
|----------|------------|----------|-----------|
| **Interactive UI** (default) | 10 | 60-120s | Fast feedback, good fairness |
| **Batch processing** | 50 | 300s (5m) | Higher efficiency, less fairness needed |
| **Multi-tenant/SLA** | 5 | 30-60s | Strict fairness guarantees |

**Metrics to Track:**
- Model swaps per job
- Time spent on model loading vs. processing
- Fairness metric: Standard deviation of GPU time per job
- Photos processed per model swap (amortization efficiency)
- Time to first result per job (responsiveness)

## Recommendation

For initial implementation, use the **Interactive UI profile**:
```toml
min_photos_before_swap = 10
max_time_before_swap = "120s"
```

**Why these values:**
- ✅ Model load overhead (10s) amortized over 10+ photos (10% or less overhead)
- ✅ Temporal fairness: All jobs see progress every ~2 minutes
- ✅ Good user experience: Responsive polling updates
- ✅ Works well for typical photography workloads (20-200 photos per job)
- ✅ Adapts automatically to photo complexity without tuning

Both thresholds should be exposed as configuration options for users to tune based on their specific needs and hardware.

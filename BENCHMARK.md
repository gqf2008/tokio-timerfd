# Timer benchmark

This benchmark measures end-to-end scheduling error, not raw syscall cost.

- `delay`: elapsed time after the requested deadline.
- `interval`: `(actual tick delta - requested period)`.
- `delay-queue`: elapsed time after each queued deadline while 1,000 entries
  share one native timer.
- Values are microseconds. Negative values mean the callback ran early.
- Results include timer creation for `Delay`; `Interval` and `DelayQueue`
  reuse one timer.

Run it with:

```sh
BENCH_ITERATIONS=1000 BENCH_QUEUE_ENTRIES=5000 cargo bench --bench timer_latency
```

The defaults are 500 iterations at 100us, 500us, 1ms, 5ms, and 10ms, plus
1,000 queue entries spaced 500us apart. Use a release build, avoid concurrent
workloads, and repeat on target hardware.

## Sample: Windows x86_64

Collected on 2026-09-10 with Rust 1.97.1. The process was not CPU-pinned.
These results describe only this machine and are not performance guarantees.

```text
backend,period_us,iterations,mean_us,p50_us,p95_us,p99_us,max_us,min_us
tokio-timerfd-delay,100,500,567.615,424.900,907.100,916.500,945.900,405.300
tokio-timerfd-interval,100,500,880.242,900.100,918.200,930.100,996.300,-99.900
tokio-time-delay,100,500,15535.879,15545.300,15935.100,15999.100,16020.900,14611.300
tokio-time-interval,100,500,15632.079,15762.800,15923.000,15989.900,16013.800,14579.900
tokio-timerfd-delay,500,500,465.933,499.300,513.100,523.300,558.200,20.700
tokio-timerfd-interval,500,500,468.165,499.900,513.500,524.300,533.900,-498.600
tokio-time-delay,500,500,15101.361,15132.400,15533.900,15584.800,15623.400,2469.900
tokio-time-interval,500,500,14843.782,15443.800,16474.600,28897.100,35358.000,-499.900
tokio-timerfd-delay,1000,500,764.104,988.400,1007.900,1017.600,1021.500,34.100
tokio-timerfd-interval,1000,500,0.236,0.100,13.700,26.200,32.400,-29.600
tokio-time-delay,1000,500,14618.372,14663.300,15028.300,15094.700,15124.300,5126.100
tokio-time-interval,1000,500,14686.064,14750.600,15043.400,15106.500,15131.100,13749.700
tokio-timerfd-delay,5000,500,955.091,1000.900,1018.400,1041.900,1657.900,34.500
tokio-timerfd-interval,5000,500,38.360,1.700,559.200,2950.100,8719.600,-3179.300
tokio-time-delay,5000,500,10621.909,10587.800,11041.400,11110.300,15945.300,9605.500
tokio-time-interval,5000,500,10687.756,10753.300,11026.300,11092.900,11118.500,9776.600
tokio-timerfd-delay,10000,500,717.630,861.700,1011.400,1020.800,1031.000,15.500
tokio-timerfd-interval,10000,500,-1.307,2.100,539.600,809.100,957.200,-1004.200
tokio-time-delay,10000,500,5615.715,5579.100,6037.800,6104.700,8750.400,4637.700
tokio-time-interval,10000,500,5653.185,5635.200,6026.400,6097.900,6129.200,4632.800
tokio-timerfd-delay-queue,500,1000,260.533,261.500,491.100,513.400,560.400,0.500
```

The sub-millisecond `Delay` result shows that this Windows environment has an
approximately 0.4-1ms scheduling floor even with the high-resolution timer
enabled. The reusable `Interval` path performs substantially better at 1ms and
above. Linux and BSD numbers must be collected on their native systems before
making cross-platform precision claims.

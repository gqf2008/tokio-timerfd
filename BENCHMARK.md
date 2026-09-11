# Timer benchmark

This benchmark measures end-to-end scheduling behavior, not raw syscall cost.

- `delay`: absolute lateness after the requested deadline.
- `delay-create`: time spent constructing the timer before it is armed.
- `interval`: `(actual tick delta - requested period)`. Low values do not imply accurate absolute wakeups.
- `delay-queue`: absolute lateness for 1,000 entries multiplexed over one timer.
- Values are microseconds. Negative values mean the callback ran early.
- `Interval` measures tick-to-tick period error. A fixed phase offset is not included, so this metric does not establish one-shot deadline accuracy.

Run it with:

```sh
BENCH_ITERATIONS=1000 BENCH_QUEUE_ENTRIES=5000 cargo bench --bench timer_latency
```

The defaults are 500 iterations at 100us, 500us, 1ms, 5ms, and 10ms, plus
1,000 queue entries spaced 500us apart. Use a release build, avoid concurrent
workloads, and repeat on target hardware.

## Sample: Windows x86_64

Collected on 2026-09-11 with Rust 1.97.1. The process was not CPU-pinned.
These results describe only this machine and are not performance guarantees.

```text
backend,period_us,iterations,mean_us,p50_us,p95_us,p99_us,max_us,min_us
tokio-timerd-delay,100,500,899.950,899.700,909.900,916.500,937.600,805.000
tokio-timerd-delay-create,100,500,2.742,2.700,3.200,3.900,7.800,2.100
tokio-timerd-interval,100,500,860.146,899.400,910.400,913.900,931.600,-99.900
tokio-time-delay,100,500,15752.914,15901.900,15916.400,15927.100,16005.300,860.700
tokio-time-interval,100,500,15788.076,15902.700,15916.000,15928.300,16031.400,14853.000
tokio-timerd-delay,500,500,499.220,500.100,509.900,519.400,536.800,38.200
tokio-timerd-delay-create,500,500,2.775,2.700,3.300,4.400,7.500,2.000
tokio-timerd-interval,500,500,470.163,499.900,509.500,515.300,549.300,-498.900
tokio-time-delay,500,500,15362.594,15502.000,15515.000,15566.500,15601.300,4465.700
tokio-time-interval,500,500,15373.570,15502.100,15519.800,15552.200,15617.700,14438.600
tokio-timerd-delay,1000,500,723.025,513.900,1008.200,1013.800,1017.800,19.300
tokio-timerd-delay-create,1000,500,2.828,2.800,3.300,3.600,20.400,2.000
tokio-timerd-interval,1000,500,-0.749,0.300,11.500,17.800,982.800,-997.600
tokio-time-delay,1000,500,14840.520,15002.100,15015.500,15032.700,15087.200,1465.600
tokio-time-interval,1000,500,14865.417,15001.900,15017.600,15067.500,15109.200,13991.000
tokio-timerd-delay,5000,500,897.117,999.900,1007.000,1013.200,1024.000,37.900
tokio-timerd-delay-create,5000,500,2.916,2.800,3.300,4.200,13.200,2.000
tokio-timerd-interval,5000,500,0.892,1.300,15.100,502.500,570.600,-503.700
tokio-time-delay,5000,500,10852.520,11001.500,11019.500,11055.900,11132.700,1956.400
tokio-time-interval,5000,500,10871.526,11002.200,11018.900,11091.600,11129.200,9984.600
tokio-timerd-delay,10000,500,796.254,997.700,1009.900,1017.100,1038.200,27.600
tokio-timerd-delay-create,10000,500,2.727,2.600,3.200,4.800,11.700,1.900
tokio-timerd-interval,10000,500,-0.747,3.300,39.400,512.700,679.200,-994.300
tokio-time-delay,10000,500,5906.313,6001.900,6017.200,6080.700,13973.300,4848.700
tokio-time-interval,10000,500,5908.010,6002.100,6015.200,6047.600,6115.000,4812.100
tokio-timerd-delay-queue,500,1000,448.854,272.700,745.400,756.200,772.500,120.100
```

`Delay::new` costs about 2.7us on this machine, so timer creation is not the dominant error. Absolute Windows wakeups show an approximately 0.5-1ms scheduling floor even with the high-resolution waitable timer enabled. Reused `Interval` timers can keep tight tick-to-tick deltas, but that does not remove the fixed phase offset. Linux and BSD numbers must be collected on their native systems before making cross-platform precision claims.

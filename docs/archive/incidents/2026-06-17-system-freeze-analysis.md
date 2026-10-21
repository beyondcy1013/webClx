# 2026-06-17 16:50 System Freeze Analysis

> 历史事故记录：用于追溯当时证据，不代表当前系统状态。

## Conclusion

The power key was a recovery action after the machine was already unresponsive. The first confirmed bad state was PostgreSQL and storage/log I/O saturation beginning around 16:16-16:18 CST, not webClx.

Primary culprit chain:

- `stockScreener` jiepan PostgreSQL paths issued expensive `stock_info` queries:
  - `SELECT ... COUNT(*) OVER() ... FROM stock_info WHERE time >= $1 ORDER BY time DESC LIMIT $2`
  - `INSERT INTO stock_info_jiepan_outbox ... SELECT ... FROM stock_info ... LIMIT 10000`
- PostgreSQL then showed connection/authentication timeouts for other services and very slow checkpoints.
- Redis AOF/RDB persistence started reporting `Asynchronous AOF fsync is taking too long (disk is busy?)`.
- `systemd-journald` hit its watchdog and dumped core while the persistent journal was near its 1 GB cap.
- Docker/containerd became unavailable and dockerd repeatedly logged containerd socket timeouts.
- `sub2api`, `sub2freeApi`, `new-api`, `mixapi`, `signIn`, clash/frpc then emitted many downstream timeout errors, amplifying the log and I/O pressure.

## Key Evidence

- PostgreSQL log `/home/data/postgresql/18/main/log/postgresql-Wed.log`:
  - `16:16:52`: `stock_info` `COUNT(*) OVER()` query canceled due to statement timeout.
  - `16:17:24`: `stock_info_jiepan_outbox` seed `INSERT ... SELECT ... LIMIT 10000` canceled due to statement timeout.
  - `16:17:57` onward: `sub2api` and `sub2freeApi` authentication timeouts.
  - `16:19:21-16:22:18`: checkpoint wrote only 85 buffers but took 177 seconds.
- Redis entries in `/var/log/messages`:
  - Normal saves at `16:15` and `16:16` were about 0.1 seconds.
  - `16:18:06` first `Asynchronous AOF fsync is taking too long (disk is busy?)`.
  - Repeated AOF fsync warnings through `16:50-16:51`.
- journald entries in `/var/log/messages`:
  - `16:21:57`: `systemd-journald.service: Watchdog timeout (limit 3min)!`
  - `16:21:09`: journal file was `912.0M`, max `1.0G`.
- Docker entries in `/var/log/messages`:
  - `16:20:14`: `killing and restarting containerd`.
  - Later repeated `failed connecting to containerd` / `context deadline exceeded`.
- Shutdown entries:
  - `16:51:54`: `Power key pressed short`.
  - This is after the earlier I/O and DB failures, so it is not the cause.

## Useful Follow-up Checks

- For SQL shape, run `EXPLAIN` without `ANALYZE` against the `stock_info` source catalog and outbox seed queries.
- Check `stockScreener/src/reason/jiepan_pg.rs` and `stockScreener/src/reason/jiepan_pg_index.rs` before changing query behavior.
- Treat `sub2api`/`sub2freeApi` log storms as downstream amplification unless new evidence shows they triggered the initial PostgreSQL saturation.

## 2026-06-20 Recheck After Optimizations

- PostgreSQL checkpoints on `2026-06-20` are back in the normal few-second range; the earlier 177-second checkpoint stall did not recur.
- I still see scattered network/proxy timeouts and subscription pull failures in the logs, but not the earlier full chain of PostgreSQL timeout -> Redis AOF fsync delay -> journald/containerd pressure -> system unresponsiveness.
- The likely improvement is real, but the remaining noise suggests the system is not clean enough to declare the entire path fixed; keep watching for renewed `stock_info` statement timeouts or AOF fsync warnings.

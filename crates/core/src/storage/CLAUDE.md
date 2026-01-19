# Storage Module

Write-ahead log for durable state persistence with crash recovery.

## File Layout

```
.oj/
├── wal.jsonl                           # Write-ahead log (append-only)
└── snapshots/
    └── 00000042-20260115123456.json    # {sequence}-{timestamp}
```

## Storage Invariants

```
INVARIANT: Writes are atomic (no partial states)
INVARIANT: Reads return consistent state or error
INVARIANT: fsync() completes before returning success
INVARIANT: Sequence numbers never decrease or repeat
INVARIANT: Every entry is verified by checksum on read
INVARIANT: Replaying the same entries produces identical state
```

## Entry Format

Each WAL entry is a single line of JSON:

```json
{"sequence":1,"timestamp_micros":1705123456789000,"machine_id":"m1","operation":{...},"checksum":12345}
```

CRC32 checksum is calculated over the JSON-serialized operation.

## Corruption Handling

- **Truncated writes**: Detected by JSON parse failure at end of file
- **Bit flips**: Detected by CRC32 checksum mismatch
- **Recovery**: Truncates WAL at last valid entry

## Operation Types

| Category | Operations |
|----------|-----------|
| Pipeline | create, transition, delete |
| Task | create, transition, delete |
| Workspace | create, transition, delete |
| Queue | push, claim, complete, fail, release, delete, tick |
| Lock | acquire, release, heartbeat |
| Semaphore | acquire, release, heartbeat |
| Session | create, transition, heartbeat, delete |
| Event | emit |
| Snapshot | taken (marker) |

## Landing Checklist

- [ ] New fields have `#[serde(default)]`
- [ ] New operations added to Operation enum
- [ ] State roundtrips through JSON correctly
- [ ] Tests cover serialization roundtrip
- [ ] Checksum verification tested

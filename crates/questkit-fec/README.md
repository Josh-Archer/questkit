# questkit-fec

Reed-Solomon and XOR packet erasure coding for lossy transports. The crate is
transport independent: callers provide source payloads and decide how to send,
identify, and buffer the data and parity packets.

The parity packets include a 20-byte `FecHeader`. Its magic values and
big-endian field layout match DeskQuest's existing transport format:

```text
magic[4] | block_id:u64 | source_count:u16 | parity_count:u16
         | parity_index:u16 | symbol_size:u16
```

All integers are big-endian. Each source payload is encoded with a 2-byte
big-endian length prefix and zero padding to the largest source payload in that
block. Keep this framing stable when interoperating with DeskQuest.

```toml
[dependencies]
questkit-fec = "0.1"
bytes = "1"
```

```rust
use bytes::Bytes;
use questkit_fec::ReedSolomonFec;
use std::collections::HashMap;

let source = vec![Bytes::from_static(b"one"), Bytes::from_static(b"two")];
let parity = ReedSolomonFec::encode(7, &source, 1)?;
let mut received = HashMap::new();
received.insert(0, source[0].clone());
let recovered = ReedSolomonFec::decode(7, 2, 1, received, &[(0, parity[0].clone())])?;
assert_eq!(recovered, source);
# Ok::<(), questkit_fec::FecError>(())
```

`ReedSolomonFec` can recover up to the number of available parity packets in a
block (at most 256 source-plus-parity symbols). `XorFec` provides one parity
packet and recovers one missing source. Both decoders return all source payloads
in source-index order.

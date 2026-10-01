# FLORA RS-485 Protocol

`PROTOCOL_VERSION = "flora-rs485-v1"`. This file is canonical; the embedded copies in `node/`, `aggregator/`, and `provision/` must match it. `cloud/protocol_vectors.json` is the byte-level source of truth.

## Physical and transaction rules

- Half-duplex, 115200 baud, 8 data bits, no parity, 1 stop bit (8N1).
- The master asserts DE while transmitting, waits for the final stop bit, releases DE, then waits at least 2 ms before accepting a reply.
- The master permits one outstanding request. Responses use the request sequence number; retries reuse it. The master must resolve the prior transaction before issuing another sequence for the same node.
- A slave must not execute a duplicate `(master, sequence, command)` request twice; it returns the prior result. This rule also applies to side-effecting operations such as calibration.
- Broadcast frames never receive replies. `PING` is informational only; discovery uses addressed `GET_INFO` requests.

## Frame layout

All multi-byte integers are big-endian. The frame is:

| Field | Size | Meaning |
|---|---:|---|
| SOF | 1 | `0xAA` |
| ADDR | 1 | `0x00` broadcast; `0x01` aggregator identity; `0x02..0xFE` nodes; `0xFF` reserved/invalid. Requests address a node; replies identify the responding node. |
| CMD | 1 | Opcode below |
| SEQ | 2 | Master transaction number; echoed unchanged in the response |
| LEN | 1 | Payload length, `0..64` |
| PAYLOAD | LEN | Command-specific bytes |
| CRC16 | 2 | CRC-16/CCITT-FALSE over `ADDR` through the final payload byte |
| EOF | 1 | `0x55` |

CRC parameters: polynomial `0x1021`, initial value `0xFFFF`, no reflection, xor-out `0x0000`; transmit the CRC most-significant byte first. The check value for ASCII `123456789` is `0x29B1`. The maximum frame is 73 bytes. Bytes `0xAA` and `0x55` may occur in payloads; parsing is length-driven, not delimiter-scanning.

A receiver rejects reserved address `0xFF`, payloads over 64 bytes, non-exact frame lengths, bad SOF/EOF, and bad CRC. It must discard a rejected frame and resume at the next SOF.

## Opcodes and payloads

| Code | Name | Direction | Payload |
|---:|---|---|---|
| `0x01` | `POLL` | master → node | none; node replies `DATA` |
| `0x02` | `SET_VALVE` | master → node | `state:u8` (`0` closed, `1` open), `duration_s:u16` |
| `0x03` | `SET_THRESHOLD` | master → node | `dry:u16`, `wet:u16`, both per mille `0..1000` |
| `0x04` | `CALIBRATE` | master → node | `mode:u8` (`0` dry reference, `1` wet reference) |
| `0x05` | `PING` | master → broadcast | none; no reply |
| `0x06` | `SET_ADDRESS` | master → node | `new_addr:u8`, valid range `0x02..0xFE` |
| `0x07` | `GET_INFO` | master → node | none; node replies `INFO` |
| `0x80` | `ACK` | node → master | `completed_cmd:u8` |
| `0x81` | `NACK` | node → master | `rejected_cmd:u8`, `reason:u8` |
| `0x82` | `DATA` | node → master | 27-byte `SensorFrame` |
| `0x83` | `INFO` | node → master | `node_uuid:[u8;8]`, `address:u8`, `fw_ver:u16` (11 bytes) |

`DATA` is the response to `POLL`; `INFO` is the response to `GET_INFO`; other successful commands receive `ACK`. A response's ADDR is the responding node address and its SEQ equals the request SEQ. NACK reason values are `1=unsupported`, `2=invalid_payload`, `3=busy`, `4=safety_rejected`.
Unknown opcodes receive `NACK` with reason `1`; the maximum-payload vector uses an unknown opcode to exercise framing limits only.

`SensorFrame` fields, in wire order:

| Field | Type | Units |
|---|---|---|
| `node_uuid` | `[u8;8]` | stable provisioned identity |
| `fw_ver` | `u16` | packed firmware version |
| `moisture` | `u16` | per mille, `0..1000` |
| `temperature` | `i16` | deci-degrees Celsius |
| `humidity` | `u16` | per mille, `0..1000` |
| `valve_state` | `u8` | `0` closed, `1` open |
| `battery_mv` | `u16` | millivolts |
| `seq` | `u32` | monotonic node reading sequence |
| `uptime_s` | `u32` | seconds since boot |

## Discovery and addressing

The provisioner assigns each node a unique address in `0x02..0xFE`. At boot and periodically, the aggregator scans that range with addressed `GET_INFO` requests and builds its `node_uuid → address` map. It must not broadcast a request that makes all nodes reply simultaneously. A duplicate address causes a response collision and is reported as an address fault; resolve it through the provisioner by assigning distinct addresses. Automatic reassignment is not safe when two devices share the same address because the master cannot identify which device answered.

## Golden vectors and versioning

`protocol_vectors.json` contains valid and invalid byte-level vectors. Node, aggregator, and provisioner tests must encode/decode every valid vector and reject every invalid vector. Their CI downloads the cloud vector file at a pinned cloud commit and compares it with the checked-in copy. A wire-format change requires a new `PROTOCOL_VERSION`, updated copies, and new vectors; never update a vector to hide a codec mismatch.

## Changelog

- `flora-rs485-v1`: initial versioned frame; adds a 16-bit transaction sequence, caps payloads at 64 bytes, uses 16-bit thresholds for the `0..1000` sensor range, and replaces collision-prone broadcast discovery with address scanning.

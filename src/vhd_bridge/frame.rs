//! vhd-machine-auth-bridge: 4-byte little-endian length prefix +
//! JSON-payload frame codec, shared by all four bridge frame types.
//!
//! The wire format mirrors the framing style already used by
//! `src/ipc.rs` (4-byte length prefix); see design.md §"帧编解码".
//! Only this module owns the codec — callers stay above the byte layer.

use hbb_common::tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum permitted JSON-payload byte length. Frame headers exceeding
/// this trigger `io::Error::InvalidData` (per Requirement 13.4 / design
/// §"帧编解码"). 64 KiB matches the cap shared with `src/ipc.rs`.
pub(super) const MAX_FRAME_BYTES: usize = 64 * 1024;

/// Read one full frame off `r` into `scratch`, returning a slice into
/// the same buffer for the JSON payload bytes. The previous contents of
/// `scratch` are discarded. Returns `InvalidData` if the length prefix
/// claims more than `MAX_FRAME_BYTES`.
pub(super) async fn read_frame<'a, R>(r: &mut R, scratch: &'a mut Vec<u8>) -> io::Result<&'a [u8]>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "vhd_bridge: frame too large",
        ));
    }
    scratch.clear();
    scratch.resize(len, 0);
    r.read_exact(scratch.as_mut_slice()).await?;
    Ok(scratch.as_slice())
}

/// Write `payload` as a single frame: 4-byte little-endian length
/// prefix followed by the payload bytes. Returns an error if the
/// payload would exceed `MAX_FRAME_BYTES`.
pub(super) async fn write_frame<W>(w: &mut W, payload: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    if payload.len() > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "vhd_bridge: payload exceeds MAX_FRAME_BYTES",
        ));
    }
    let len = payload.len() as u32;
    w.write_all(&len.to_le_bytes()).await?;
    w.write_all(payload).await?;
    w.flush().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
//
// Task 5.3 (Property 1, codec half): the byte-layer round-trip and the
// `MAX_FRAME_BYTES` rejection check live next to the codec they
// exercise. The JSON-schema half of Property 1 lives in
// `protocol.rs::tests` and runs through this same codec end-to-end.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hbb_common::tokio;
    use proptest::prelude::*;

    /// Build a fresh single-thread current-thread runtime for one proptest
    /// case. We intentionally do not reuse a runtime across cases so each
    /// invocation is hermetic and the test can be run from any caller
    /// context (including `cargo test` which has no ambient runtime).
    fn run_blocking<F, T>(fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("vhd_bridge::frame tests: build current-thread runtime")
            .block_on(fut)
    }

    /// Generate a payload sized in `[0, MAX_FRAME_BYTES]`. The upper bound
    /// is the codec's contract limit; oversized payloads are tested
    /// separately by `payload_oversize_rejected`.
    fn payload_strategy() -> impl Strategy<Value = Vec<u8>> {
        prop::collection::vec(any::<u8>(), 0..=MAX_FRAME_BYTES)
    }

    proptest! {
        // Feature: vhd-machine-auth-bridge, Property 1 (codec half):
        // For any payload of size ≤ MAX_FRAME_BYTES, write_frame followed
        // by read_frame yields the exact same bytes.
        #![proptest_config(ProptestConfig {
            cases: 100,
            // Generating up to 64 KiB per case; the default would be fine
            // but we set it explicitly so size growth is bounded across
            // shrink steps.
            max_shrink_iters: 256,
            ..ProptestConfig::default()
        })]

        #[test]
        fn frame_round_trip(payload in payload_strategy()) {
            let result: io::Result<Vec<u8>> = run_blocking(async {
                let mut buf = Vec::<u8>::new();
                write_frame(&mut buf, &payload).await?;
                let mut scratch = Vec::new();
                let mut reader: &[u8] = &buf[..];
                let read = read_frame(&mut reader, &mut scratch).await?;
                Ok(read.to_vec())
            });
            let read = result.expect("vhd_bridge::frame round-trip must succeed");
            prop_assert_eq!(read, payload);
        }

        #[test]
        fn declared_length_over_cap_rejected(extra in 1u32..=4096u32) {
            // Build a frame header whose declared length exceeds the cap
            // by `extra`. The codec MUST reject before reading any
            // payload bytes — we deliberately supply zero payload bytes
            // to confirm the rejection happens at the header check.
            let bad_len = (MAX_FRAME_BYTES as u32).saturating_add(extra);
            let header = bad_len.to_le_bytes();
            let kind = run_blocking(async {
                let mut scratch = Vec::new();
                let mut reader: &[u8] = &header[..];
                read_frame(&mut reader, &mut scratch)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.kind())
            });
            prop_assert_eq!(kind, Err(io::ErrorKind::InvalidData));
        }
    }

    #[test]
    fn declared_length_at_cap_is_accepted() {
        // Boundary: a frame whose declared length is exactly
        // `MAX_FRAME_BYTES` is permitted; only `> MAX_FRAME_BYTES` is
        // rejected. This is the inclusive-cap contract documented above
        // `MAX_FRAME_BYTES`.
        let payload = vec![0xAAu8; MAX_FRAME_BYTES];
        let result = run_blocking(async {
            let mut buf = Vec::<u8>::new();
            write_frame(&mut buf, &payload).await?;
            let mut scratch = Vec::new();
            let mut reader: &[u8] = &buf[..];
            let read = read_frame(&mut reader, &mut scratch).await?;
            io::Result::Ok(read.len())
        });
        assert_eq!(result.unwrap(), MAX_FRAME_BYTES);
    }

    #[test]
    fn declared_length_one_over_cap_rejected() {
        // The exact boundary case: MAX_FRAME_BYTES + 1.
        let bad_len = (MAX_FRAME_BYTES as u32) + 1;
        let header = bad_len.to_le_bytes();
        let result = run_blocking(async {
            let mut scratch = Vec::new();
            let mut reader: &[u8] = &header[..];
            read_frame(&mut reader, &mut scratch)
                .await
                .map(|_| ())
        });
        let err = result.expect_err("MAX_FRAME_BYTES + 1 must be rejected");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn write_frame_rejects_oversize_payload() {
        // Symmetric guard on the writer: refusing to emit a header that
        // a conforming peer will reject prevents the worker from
        // half-flushing a doomed frame.
        let payload = vec![0u8; MAX_FRAME_BYTES + 1];
        let result = run_blocking(async {
            let mut buf = Vec::<u8>::new();
            write_frame(&mut buf, &payload).await
        });
        let err = result.expect_err("oversize payload must be refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}

use bytes::BytesMut;
use questkit_fec::{FecHeader, FecScheme, ReedSolomonFec, XorFec, FEC_HEADER_SIZE};

#[test]
fn reed_solomon_header_bytes_match_deskquest_wire_format() {
    let header = FecHeader::new(
        FecScheme::ReedSolomon,
        0x0102_0304_0506_0708,
        10,
        2,
        1,
        0x1234,
    );
    let mut encoded = BytesMut::new();
    header.encode(&mut encoded);

    assert_eq!(encoded.len(), FEC_HEADER_SIZE);
    assert_eq!(
        &encoded[..],
        &[
            b'Q', b'F', b'R', b'S', 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x0a,
            0x00, 0x02, 0x00, 0x01, 0x12, 0x34,
        ]
    );

    let mut input = encoded.as_ref();
    assert_eq!(FecHeader::decode(&mut input), Ok(header));
    assert!(input.is_empty());
}

#[test]
fn xor_header_retains_its_distinct_magic() {
    let header = FecHeader::new(FecScheme::Xor, 9, 3, 1, 0, 4);
    let mut encoded = BytesMut::new();
    header.encode(&mut encoded);
    assert_eq!(&encoded[..4], b"QFXR");
}

#[test]
fn parity_packets_recover_variable_length_payloads() {
    let source = vec![
        bytes::Bytes::from_static(b"short"),
        bytes::Bytes::from_static(b"a longer source payload"),
        bytes::Bytes::from_static(b""),
    ];

    let rs = ReedSolomonFec::encode(12, &source, 2).unwrap();
    let recovered_rs = ReedSolomonFec::decode(
        12,
        source.len(),
        2,
        [(0, source[0].clone())].into_iter().collect(),
        &[(0, rs[0].clone()), (1, rs[1].clone())],
    )
    .unwrap();
    assert_eq!(recovered_rs, source);

    let xor = XorFec::encode(13, &source).unwrap();
    let recovered_xor = XorFec::decode(
        13,
        source.len(),
        [(0, source[0].clone()), (2, source[2].clone())]
            .into_iter()
            .collect(),
        Some(&xor),
    )
    .unwrap();
    assert_eq!(recovered_xor, source);
}

#[test]
fn malformed_and_truncated_headers_are_rejected() {
    let mut truncated: &[u8] = b"QFRS";
    assert!(FecHeader::decode(&mut truncated).is_err());

    let unknown = [0u8; FEC_HEADER_SIZE];
    assert!(FecHeader::decode(&mut &unknown[..]).is_err());
}

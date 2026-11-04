//! Preserve the numeric input contract while removing lexical-core (issue #1239).
//!
//! Test the real MSH entry point, including non-finite spellings the previous
//! lexical backend accepted. Binary parsing keeps both byte orders.

fn ascii_scalar(text: &str) -> Result<f64, ()> {
    let source = format!(
        "$MeshFormat\n4.1 0 8\n$EndMeshFormat\n\
         $Nodes\n1 1 1 1\n3 1 0 1\n1\n{text} 0 0\n$EndNodes\n"
    );
    mshio::parse_msh_bytes(source.as_bytes())
        .map(|mesh| mesh.data.nodes.unwrap().node_blocks[0].nodes[0].x)
        .map_err(|_| ())
}

#[test]
fn decimal_boundaries_preserve_f64_bits() {
    for input in [
        "0",
        "-0",
        "+0",
        "1e-300",
        "4.9406564584124654e-324",
        "2.2250738585072014e-308",
        "1.7976931348623157e308",
        "0.1",
        "9007199254740993",
        "1.00000000000000011102230246251565404236316680908203125",
        "-7.1234567890123456789e-42",
        ".5",
        "5.",
        "1e999",
        "-1e999",
        "1e-999",
    ] {
        assert_eq!(
            ascii_scalar(input).unwrap().to_bits(),
            input.parse::<f64>().unwrap().to_bits(),
            "{input}"
        );
    }
}

#[test]
fn signed_special_values_keep_every_case_variant() {
    for (word, value) in [
        ("nan", f64::NAN),
        ("inf", f64::INFINITY),
        ("infinity", f64::INFINITY),
    ] {
        for mask in 0..(1usize << word.len()) {
            let case: String = word
                .bytes()
                .enumerate()
                .map(|(index, byte)| {
                    let ch = char::from(byte);
                    if (mask >> index) & 1 == 1 {
                        ch.to_ascii_uppercase()
                    } else {
                        ch
                    }
                })
                .collect();
            for (sign, negative) in [("", false), ("+", false), ("-", true)] {
                let input = format!("{sign}{case}");
                let expected = if negative { -value } else { value };
                assert_eq!(
                    ascii_scalar(&input).unwrap().to_bits(),
                    expected.to_bits(),
                    "{input}"
                );
            }
        }
    }
}

#[test]
fn malformed_numeric_tokens_are_rejected() {
    for input in [
        "",
        "abc",
        "1e",
        "1e+",
        "--1",
        "0x1p2",
        "1.0junk",
        "NaNjunk",
        "infinityjunk",
        "1.e",
        ".e2",
        "1_0",
        "0xff",
        "0b10",
        "0o10",
        "++1",
    ] {
        assert!(ascii_scalar(input).is_err(), "accepted {input:?}");
    }
}

#[test]
fn finite_bit_patterns_round_trip() {
    let mut bits = 0xa124_bb45_8f00_3651u64;
    for _ in 0..8192 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let value = f64::from_bits(bits);
        if value.is_finite() {
            assert_eq!(ascii_scalar(&value.to_string()).unwrap().to_bits(), bits);
        }
    }
}

#[test]
fn binary_node_coordinates_keep_both_byte_orders() {
    for big_endian in [false, true] {
        let mut source = b"$MeshFormat\n4.1 1 8\n".to_vec();
        source.extend(if big_endian {
            1i32.to_be_bytes()
        } else {
            1i32.to_le_bytes()
        });
        source.extend(b"\n$EndMeshFormat\n$Nodes\n");
        // One block and one node; tag range 1..=1.
        for value in [1u64, 1, 1, 1] {
            source.extend(if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            });
        }
        // Volume entity 1, non-parametric coordinates.
        for value in [3i32, 1, 0] {
            source.extend(if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            });
        }
        // Block count and the node's tag.
        for value in [1u64, 1] {
            source.extend(if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            });
        }
        let expected = [0.1f64, -0.0, 1e-300];
        for value in expected {
            source.extend(if big_endian {
                value.to_be_bytes()
            } else {
                value.to_le_bytes()
            });
        }
        source.extend(b"\n$EndNodes\n");
        let mesh = mshio::parse_msh_bytes(&source).unwrap();
        let node = &mesh.data.nodes.as_ref().unwrap().node_blocks[0].nodes[0];
        for (actual, expected) in [node.x, node.y, node.z].into_iter().zip(expected) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
    }
}

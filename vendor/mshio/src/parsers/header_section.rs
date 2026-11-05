use std::str;

use nom::character::complete::digit1;
use nom::combinator::map;
use nom::number::complete as numbers;
use nom::number::Endianness;
use nom::sequence::{delimited, preceded};
use nom::IResult;

use crate::error::{
    always_error, context, make_error, MapMshError, MshParserError, MshParserErrorKind,
};
use crate::mshfile::{MshFloatT, MshHeader, MshIntT, MshUsizeT};
use crate::parsers::num_parser_traits::{ParsesFloat, ParsesInt, ParsesSizeT};
use crate::parsers::num_parsers;
use crate::parsers::{br, sp, verify_or};

pub(crate) fn parse_header_section<'a>(
    input: &'a [u8],
) -> IResult<
    &'a [u8],
    (
        MshHeader,
        impl ParsesSizeT<u64> + ParsesInt<i32> + ParsesFloat<f64>,
    ),
    MshParserError<&'a [u8]>,
> {
    // TODO: Replace this expect
    let from_u8 =
        |items| str::FromStr::from_str(str::from_utf8(items).expect("Cannot parse UTF8 to digits"));

    let (input, version) = verify_or(
        num_parsers::parse_double,
        |&version| version == 4.1,
        always_error(MshParserErrorKind::UnsupportedMshVersion),
    )(input)?;

    let (input, file_type) = context(
        "file type flag",
        verify_or(
            preceded(sp, map(digit1, from_u8)),
            |file_type| *file_type == Ok(0) || *file_type == Ok(1),
            context(
                "Invalid file type (expected 0 for ASCII or 1 for binary)",
                always_error(MshParserErrorKind::InvalidFileHeader),
            ),
        ),
    )(input)?;
    let (input, data_size) = context(
        "data size",
        verify_or(
            delimited(sp, map(digit1, from_u8), br),
            |data_size| data_size.is_ok(),
            context(
                "Invalid data size (not representable as an integer)",
                always_error(MshParserErrorKind::InvalidFileHeader),
            ),
        ),
    )(input)?;

    // Both fields were validated above (`Ok`), so these cannot fail.
    // TODO: Replace these unwraps
    let file_type = file_type.unwrap();
    let data_size = data_size.unwrap();

    let endianness = if file_type == 1 {
        // Binary file: the num parsers only support these `size_t` widths, so reject
        // anything else here instead of letting `uint_parser` panic.
        if !matches!(data_size, 1 | 2 | 4 | 8 | 16) {
            return Err(make_error(input, MshParserErrorKind::InvalidFileHeader)
                .with_context(input, "Unsupported binary data size"));
        }

        let (_, i_be) = context("endianness test value", numbers::be_i32)(input)?;
        let (_, i_le) = context("endianness test value", numbers::le_i32)(input)?;

        if i_be == 1 {
            Some(Endianness::Big)
        } else if i_le == 1 {
            Some(Endianness::Little)
        } else {
            return Err(make_error(input, MshParserErrorKind::InvalidFileHeader)
                .with_context(input, "Unable to detect endianness of binary file"));
        }
    } else {
        // ASCII file
        None
    };

    let header = MshHeader {
        version,
        file_type,
        size_t_size: data_size as usize,
        int_size: 4,
        float_size: 8,
        endianness,
    };
    let parsers = num_parsers_from_header(&header);

    Ok((input, (header, parsers)))
}

pub(crate) fn num_parsers_from_header<'a, U: MshUsizeT, I: MshIntT, F: MshFloatT>(
    header: &'a MshHeader,
) -> impl ParsesSizeT<U> + ParsesInt<I> + ParsesFloat<F> {
    let size_t_parser = num_parsers::uint_parser::<U>(header.size_t_size, header.endianness);
    let int_parser = num_parsers::int_parser::<I>(header.int_size, header.endianness);
    let double_parser = num_parsers::float_parser::<F>(header.float_size, header.endianness);

    num_parsers::NumParsers {
        size_t_parser,
        int_parser,
        float_parser: double_parser,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_msh_bytes;

    fn header_is_err(input: &[u8]) -> bool {
        parse_header_section(input).is_err()
    }

    #[test]
    fn overflowing_data_size_returns_err_not_panic() {
        assert!(header_is_err(b"4.1 0 99999999999999999999999999\n"));
        assert!(header_is_err(b"4.1 1 99999999999999999999999999\n"));
    }

    #[test]
    fn overflowing_file_type_still_returns_err() {
        assert!(header_is_err(b"4.1 99999999999999999999999999 8\n"));
    }

    #[test]
    fn valid_ascii_header_parses() {
        let (_, (header, _)) = parse_header_section(b"4.1 0 8\n").ok().unwrap();
        assert_eq!(header.file_type, 0);
        assert_eq!(header.size_t_size, 8);
    }

    fn binary_msh(data_size: &str) -> Vec<u8> {
        let mut bytes = format!("$MeshFormat\n4.1 1 {}\n", data_size).into_bytes();
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(b"\n$EndMeshFormat\n$Nodes\n");
        bytes
    }

    #[test]
    fn unsupported_binary_data_size_is_an_error_not_a_panic() {
        for size in ["0", "3", "300"] {
            let bytes = binary_msh(size);
            assert!(
                parse_msh_bytes(&bytes).is_err(),
                "binary data size {size} should be rejected"
            );
        }
    }

    #[test]
    fn supported_binary_data_size_passes_header_validation() {
        // Header is accepted; the truncated body then fails later, but not with a panic.
        let bytes = binary_msh("8");
        let _ = parse_msh_bytes(&bytes);
    }
}

// Color scheme, use rainbow mode
// base: #ed6a5a, #18314f, #ffee93, #62bbc1, #b744b8
// #ed6a5a #3b0d07 #76190d #b02614 #e6361f #ed6a5a #f0897b #f4a69c #f8c4bd #fbe1de
// #18314f #050a10 #0a131f #0e1d2f #13273e #18314f #2b578c #447ec5 #82a9d9 #c1d4ec
// #ffee93 #514400 #a18900 #f2cd00 #ffe343 #ffee93 #fff2a9 #fff5bf #fff9d4 #fffcea
// #62bbc1 #11282a #215053 #32787d #42a0a6 #62bbc1 #82c8cd #a1d6d9 #c0e3e6 #e0f1f2
// #b744b8 #240d24 #481b49 #6c286d #903692 #b744b8 #c567c7 #d48dd5 #e2b3e3 #f1d9f1

pub mod header {
    pub const MARGIN: f32 = 0.1;
}

pub mod colors {
    use memoize::memoize;

    use std::collections::HashMap;

    use egui::Color32;
    use palette::{Gradient, LinSrgb};

    #[derive(Clone, Debug, PartialEq, Hash, Eq)]
    pub enum Kind {
        Observables,
        LaunchControl,
        RFSilence,
        Status,
    }

    #[derive(Clone, Debug, PartialEq, Hash, Eq)]
    pub enum Intensity {
        Low,
        High,
    }

    pub const OBSERVABLES: Color32 = Color32::from_rgb(0x62, 0xbb, 0xc1);
    pub const LAUNCHCONTROL: Color32 = Color32::from_rgb(0xed, 0x6a, 0x52);

    #[memoize]
    pub fn muted(color: Color32) -> Color32 {
        let muted_colors = HashMap::from([
            (OBSERVABLES, Color32::from_rgb(0x32, 0x78, 0x7d)), // #62bbc1 -> #32787d
            (LAUNCHCONTROL, Color32::from_rgb(0xb0, 0x26, 0x14)),
        ]); // #ed6a5a -> #b02614
        muted_colors[&color]
    }

    impl Into<f32> for Intensity {
        fn into(self) -> f32 {
            match self {
                Intensity::Low => 0.3,
                Intensity::High => 0.6,
            }
        }
    }

    #[memoize]
    pub fn kind_color32(kind: Kind, intensity: Intensity) -> Color32 {
        color32(kind_color(kind, intensity))
    }

    #[memoize]
    pub fn kind_color(kind: Kind, intensity: Intensity) -> LinSrgb {
        let gradient = Gradient::new(
            super::helpers::hexcolor_vec_parser(match kind {
                Kind::Observables => {
                    b"#11282a #215053 #32787d #42a0a6 #62bbc1 #82c8cd #a1d6d9 #c0e3e6 #e0f1f2"
                }
                Kind::LaunchControl => {
                    b"#240d24 #481b49 #6c286d #903692 #b744b8 #c567c7 #d48dd5 #e2b3e3 #f1d9f1"
                }
                Kind::RFSilence => {
                    b"#0e1d2f #0e1d2f #13273e #13273e #18314f #2b578c #447ec5 #82a9d9 #c1d4ec"
                }
                Kind::Status => {
                    b"#514400 #a18900 #f2cd00 #ffe343 #ffee93 #fff2a9 #fff5bf #fff9d4 #fffcea"
                    //b"#3b0d07 #76190d #b02614 #e6361f #ed6a5a #f0897b #f4a69c #f8c4bd #fbe1de"
                }
            })
            .unwrap()
            .1,
        );
        gradient.get(intensity.into())
    }

    pub fn color32(color: LinSrgb) -> Color32 {
        Color32::from_rgb(
            (color.red * 255.0) as u8,
            (color.green * 255.0) as u8,
            (color.blue * 255.0) as u8,
        )
    }
}

mod helpers {

    use nom::{
        bytes::complete::{tag, take_while_m_n},
        character::is_hex_digit,
        multi::separated_list1,
        sequence::tuple,
        IResult,
    };
    use palette::LinSrgb;

    fn unhex<'a>(c: u8) -> u8 {
        match c {
            b'0'..=b'9' => c - 48,
            b'A'..=b'F' => c - 55,
            b'a'..=b'f' => c - 55,
            _ => unreachable!(),
        }
    }

    fn hex_u32_parser(s: &[u8]) -> IResult<&[u8], (u8, u8, u8)> {
        let (rest, out) = take_while_m_n(6, 6, is_hex_digit)(s)?;
        let r = unhex(out[0]) << 4 | unhex(out[1]);
        let g = unhex(out[2]) << 4 | unhex(out[3]);
        let b = unhex(out[4]) << 4 | unhex(out[5]);
        Ok((rest, (r, g, b)))
    }

    pub fn hexcolor_parser(s: &[u8]) -> IResult<&[u8], LinSrgb> {
        let (rest, (_, (r, g, b))) = tuple((tag("#"), hex_u32_parser))(s)?;
        Ok((
            rest,
            LinSrgb::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0),
        ))
    }

    pub fn hexcolor_vec_parser(s: &[u8]) -> IResult<&[u8], Vec<LinSrgb>> {
        separated_list1(tag(" "), hexcolor_parser)(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use palette::LinSrgb;

    #[test]
    fn test_color_from_hex_string() {
        let input = b"#0000ff";
        let (rest, color) = helpers::hexcolor_parser(input).unwrap();
        assert_eq!(rest, b"");
        assert_eq!(color, LinSrgb::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn test_color_vector_from_string() {
        let input = b"#240d24 #481b49 #6c286d #903692 #b744b8 #c567c7 #d48dd5 #e2b3e3 #f1d9f1";
        let (rest, colors) = helpers::hexcolor_vec_parser(input).unwrap();
        assert_eq!(rest, b"");
        assert_eq!(
            colors,
            vec![
                LinSrgb::new(0.14117648, 0.1764706, 0.14117648),
                LinSrgb::new(0.28235295, 0.23137255, 0.28627452),
                LinSrgb::new(0.42352942, 0.15686275, 0.42745098),
                LinSrgb::new(0.5647059, 0.21176471, 0.57254905),
                LinSrgb::new(0.7176471, 0.26666668, 0.72156864),
                LinSrgb::new(0.77254903, 0.40392157, 0.78039217),
                LinSrgb::new(0.83137256, 0.6784314, 0.8352941),
                LinSrgb::new(0.8862745, 0.7019608, 0.8901961),
                LinSrgb::new(0.94509804, 0.8509804, 0.94509804),
            ]
        );
    }
}

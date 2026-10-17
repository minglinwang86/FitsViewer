use fitsview_core::*;
use std::io::Write;

fn fixture(pixels: RawPixels, bitpix: i32) -> FitsImage {
    FitsImage {
        info: ImageInfo {
            path: "test.fits".into(),
            hdu: 0,
            width: 2,
            height: 2,
            bitpix,
            bscale: 1.0,
            bzero: 0.0,
            blank: None,
            hdus: vec![],
            header: String::new(),
        },
        pixels,
    }
}
#[test]
fn packet_preserves_large_integer_and_float64() {
    let original = (1_i64 << 60) + 1;
    let image = fixture(RawPixels::I64(vec![original, -original, 0, 42]), 64);
    let copy = FitsImage::from_packet(&image.to_packet().unwrap()).unwrap();
    assert_eq!(copy.pixels.raw_string(0), original.to_string());
    let image = fixture(
        RawPixels::F64(vec![1.000000000000001, f64::NAN, -2.0, 1e200]),
        -64,
    );
    let copy = FitsImage::from_packet(&image.to_packet().unwrap()).unwrap();
    assert_eq!(copy.physical(0), Some(1.000000000000001));
    assert_eq!(copy.physical(1), None);
}
#[test]
fn physical_scaling_respects_storage_blank_and_negative_values() {
    let mut image = fixture(RawPixels::I16(vec![-32768, -2, 0, 4]), 16);
    image.info.bscale = 2.0;
    image.info.bzero = 100.0;
    image.info.blank = Some(-32768);
    assert_eq!(image.physical(0), None);
    assert_eq!(image.physical(1), Some(96.0));
    assert_eq!(image.physical(3), Some(108.0));
    image.info.bzero = 0.0;
    assert_eq!(image.physical(1), Some(-4.0));
}
#[test]
fn display_orientation_and_stretch_endpoints() {
    let image = fixture(RawPixels::U8(vec![0, 1, 2, 3]), 8);
    let rgba = render_rgba(&image, 0.0, 3.0, Stretch::Linear, false);
    assert_eq!([rgba[0], rgba[4], rgba[8], rgba[12]], [170, 255, 0, 85]);
    for stretch in Stretch::ALL {
        assert_eq!(stretch.apply(0.0), 0.0);
        assert!((stretch.apply(1.0) - 1.0).abs() < 1e-14);
        assert!(stretch.apply(0.6) > stretch.apply(0.4));
    }
}
#[test]
fn brightness_and_contrast_change_display_only() {
    let image = fixture(RawPixels::F64(vec![0.25, 0.75, f64::NAN, 0.5]), -64);
    let packet_before = image.to_packet().unwrap();
    let default = render_rgba(&image, 0.0, 1.0, Stretch::Linear, false);
    let brighter = render_rgba_with_tone(
        &image,
        0.0,
        1.0,
        Stretch::Linear,
        false,
        DisplayTone {
            bias: 0.25,
            contrast: 1.0,
        },
    );
    assert_eq!(&brighter[..4], &[95, 45, 110, 255]); // Invalid pixels keep their color.
    assert!(brighter[4] > default[4]);
    assert!(brighter[8] > default[8]);
    let strong = DisplayTone {
        bias: 0.5,
        contrast: 2.0,
    };
    assert_eq!(strong.apply(0.25), 0.0);
    assert_eq!(strong.apply(0.5), 0.5);
    assert_eq!(strong.apply(0.75), 1.0);
    let soft = DisplayTone {
        bias: 0.5,
        contrast: 0.5,
    };
    assert!(soft.apply(0.25) > 0.25 && soft.apply(0.75) < 0.75);
    for stretch in Stretch::ALL {
        let tone = DisplayTone {
            bias: 0.4,
            contrast: 1.5,
        };
        let rgba = render_rgba_with_tone(&image, 0.0, 1.0, stretch, false, tone);
        let inverted = render_rgba_with_tone(&image, 0.0, 1.0, stretch, true, tone);
        assert_eq!(
            rgba[4],
            (tone.apply(stretch.apply(0.5)) * 255.0).round() as u8
        );
        assert_eq!(inverted[4], 255 - rgba[4]);
    }
    assert_eq!(image.physical(0), Some(0.25));
    assert_eq!(image.to_packet().unwrap(), packet_before);
    assert_eq!(
        default,
        render_rgba_with_tone(
            &image,
            0.0,
            1.0,
            Stretch::Linear,
            false,
            DisplayTone::default()
        )
    );
}
#[test]
fn all_invalid_and_constant_images_have_usable_limits() {
    for pixels in [vec![f64::NAN; 4], vec![5.0; 4]] {
        let stats = Statistics::new(&fixture(RawPixels::F64(pixels), -64));
        for mode in Limits::ALL {
            let (low, high) = stats.bounds(mode);
            assert!(low.is_finite() && high.is_finite() && low < high);
        }
    }
}
#[test]
fn zscale_rejects_bright_outliers() {
    let mut values: Vec<f64> = (0..1000).map(|x| x as f64).collect();
    values.extend([1e8, 1.1e8, 1.2e8]);
    let (low, high) = zscale(&values, 0.25);
    assert!(low >= 0.0 && high < 1e6 && low < high);
}
#[test]
fn fitsrs_scaled_integer_and_truncated_file() {
    let cards = [
        "SIMPLE  =                    T",
        "BITPIX  =                   16",
        "NAXIS   =                    2",
        "NAXIS1  =                    2",
        "NAXIS2  =                    2",
        "BSCALE  =                  2.0",
        "BZERO   =                100.0",
        "BLANK   =               -32768",
        "END",
    ];
    let mut bytes = Vec::new();
    for card in cards {
        bytes.extend_from_slice(format!("{card:<80}").as_bytes());
    }
    bytes.resize(2880, b' ');
    for value in [-32768_i16, -2, 0, 4] {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&bytes).unwrap();
    let image = load_file(file.path(), None).unwrap();
    assert_eq!(image.physical(1), Some(96.0));
    assert_eq!(image.physical(0), None);
    file.as_file().set_len(2887).unwrap();
    assert!(load_file(file.path(), None)
        .unwrap_err()
        .contains("Truncated"));
}

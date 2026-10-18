//! Controlled numerical oracles for input semantics; these are not sky images.
//! 仅为误差/头信息契约的可控数值 oracle，不冒充真实科学观测。
use fitsviewer_core::*;
use std::io::Write;

fn header(cards: &[String]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in cards.iter().map(String::as_str).chain(std::iter::once("END")) {
        assert!(value.len() <= 80);
        bytes.extend_from_slice(format!("{value:<80}").as_bytes());
    }
    bytes.resize(bytes.len().div_ceil(2880) * 2880, b' ');
    bytes
}

fn append_image(bytes: &mut Vec<u8>, primary: bool, axes: &[usize], values: &[f64], extras: &[&str]) {
    let mut cards = vec![if primary { "SIMPLE  =                    T".into() } else { "XTENSION= 'IMAGE   '".into() },
        "BITPIX  =                  -64".into(), format!("NAXIS   = {}", axes.len())];
    for (index, value) in axes.iter().enumerate() { cards.push(format!("NAXIS{:<3}= {value}", index + 1)); }
    if primary { cards.push("EXTEND  =                    T".into()); }
    else { cards.extend(["PCOUNT  =                    0".into(), "GCOUNT  =                    1".into()]); }
    cards.extend(extras.iter().map(|value| (*value).into()));
    bytes.extend(header(&cards));
    for value in values { bytes.extend(value.to_be_bytes()); }
    bytes.resize(bytes.len().div_ceil(2880) * 2880, 0);
}

fn spectrum(error_name: &str, error_axes: &[usize], errors: &[f64], units: &[&str]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut extras = vec!["CRPIX1  =                    1", "CRVAL1  =                 5000", "CDELT1  =                    2", "CTYPE1  = 'LINEAR'", "CUNIT1  = 'Angstrom'"];
    extras.extend_from_slice(units);
    append_image(&mut bytes, true, &[4], &[3., -2., 5., 9.], &extras);
    let name = format!("EXTNAME = '{error_name}'");
    append_image(&mut bytes, false, error_axes, errors, &[&name]);
    bytes
}

fn same(a: &[f64], b: &[f64]) {
    assert_eq!(a.len(), b.len());
    for (actual, expected) in a.iter().zip(b) {
        assert!(actual == expected || actual.is_nan() && expected.is_nan(), "{actual} != {expected}");
    }
}

#[test]
fn explicit_error_kind_tokens_are_bounded() {
    for (token, kind) in [("auto", ErrorKind::Auto), ("sigma", ErrorKind::Sigma), ("variance", ErrorKind::Variance), ("ivar", ErrorKind::Ivar)] {
        assert_eq!(ErrorKind::parse(token), Some(kind));
        assert_eq!(kind.as_str(), token);
    }
    assert_eq!(ErrorKind::parse(" VARIANCE "), Some(ErrorKind::Variance));
    assert_eq!(ErrorKind::parse("guess-from-values"), None);
    assert_eq!(ErrorKind::default(), ErrorKind::Auto);
}

#[test]
fn unnamed_text_requires_explicit_uncertainty_semantics() {
    let text = b"5000 3 4\n5002 -2 0\n5004 5 -1\n5006 9 nan\n";
    let auto = LoadOptions { error_column: Some(2), ..Default::default() };
    assert!(load_bytes_with_options(text, "numerical-oracle.dat", auto).unwrap_err().contains("choose sigma, variance or ivar"));
    for (kind, expected, label) in [(ErrorKind::Sigma, [4., 0., f64::NAN, f64::NAN], "standard deviation"),
        (ErrorKind::Variance, [2., 0., f64::NAN, f64::NAN], "variance"),
        (ErrorKind::Ivar, [0.5, f64::NAN, f64::NAN, f64::NAN], "inverse variance")] {
        let image = load_bytes_with_options(text, "numerical-oracle.dat", LoadOptions { error_kind: kind, ..auto }).unwrap();
        same(image.spectrum.as_ref().unwrap().error.as_ref().unwrap(), &expected);
        same(&image.spectrum.as_ref().unwrap().x, &[5000., 5002., 5004., 5006.]);
        assert_eq!(image.physical(1), Some(-2.));
        assert_eq!(image.info.spectrum.as_ref().unwrap().uncertainty_kind, label);
        assert_eq!(image.info.spectrum.as_ref().unwrap().error_hdu, None);
    }
}

#[test]
fn named_auto_is_explicit_and_does_not_guess_from_substrings() {
    for (name, value, expected) in [("SIGMA", 2., 2.), ("VARIANCE", 4., 2.), ("IVAR", 0.25, 2.), ("STATERROR", 2., 2.)] {
        let text = format!("WAVE FLUX {name}\n5000 3 {value}\n5001 4 {value}\n");
        let image = load_bytes_with_options(text.as_bytes(), "numerical-oracle.txt", LoadOptions::default()).unwrap();
        assert_eq!(image.spectrum.unwrap().error.unwrap()[0], expected);
    }
    let text = b"WAVE FLUX COVARIANCE\n5000 3 4\n5001 4 4\n";
    assert!(load_bytes_with_options(text, "numerical-oracle.txt", LoadOptions { error_column: Some(2), ..Default::default() }).is_err());
}

#[test]
fn same_file_variance_hdu_preserves_flux_grid_and_packet_provenance() {
    let bytes = spectrum("VARIANCE", &[4], &[4., 9., -1., f64::NAN], &["FLXUNITS= 'erg/s/cm2/A'", "FLXERROR=                 0.03"]);
    let options = LoadOptions { hdu: Some(0), error_hdu: Some(1), ..Default::default() };
    let original = load_bytes_with_options(&bytes, "numerical-oracle.fits", options).unwrap();
    let meta = original.info.spectrum.as_ref().unwrap();
    assert_eq!(meta.error_hdu, Some(1));
    assert_eq!(meta.error_column, None);
    assert_eq!(meta.uncertainty_kind, "variance");
    assert_eq!(meta.y_unit, "erg/s/cm2/A");
    assert!(meta.has_error());
    let copy = FitsImage::from_packet(&original.to_packet().unwrap()).unwrap();
    same(&copy.spectrum.as_ref().unwrap().x, &[5000., 5002., 5004., 5006.]);
    same(copy.spectrum.as_ref().unwrap().error.as_ref().unwrap(), &[2., 3., f64::NAN, f64::NAN]);
    assert_eq!(copy.physical(1), Some(-2.));
    assert_eq!(copy.info.spectrum.as_ref().unwrap().error_hdu, Some(1));
    assert!(original.info.header.contains("FLXERROR"));
    assert!(!original.info.header.contains("EXTNAME"));

    let mut file = tempfile::Builder::new().suffix(".fits").tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let native = load_file_with_options(file.path(), options).unwrap();
    same(&native.spectrum.as_ref().unwrap().x, &original.spectrum.as_ref().unwrap().x);
    same(native.spectrum.as_ref().unwrap().error.as_ref().unwrap(), original.spectrum.as_ref().unwrap().error.as_ref().unwrap());
    assert_eq!(native.physical(1), original.physical(1));
}

#[test]
fn image_hdu_errors_require_a_distinct_matching_vector_and_known_kind() {
    let bytes = spectrum("ARBITRARY", &[4], &[4., 0., -1., f64::INFINITY], &[]);
    let options = LoadOptions { error_hdu: Some(1), ..Default::default() };
    assert!(load_bytes_with_options(&bytes, "numerical-oracle.fits", options).is_err());
    let selected = load_bytes_with_options(&bytes, "numerical-oracle.fits", LoadOptions { error_kind: ErrorKind::Ivar, ..options }).unwrap();
    same(selected.spectrum.unwrap().error.unwrap().as_slice(), &[0.5, f64::NAN, f64::NAN, f64::NAN]);
    for options in [LoadOptions { error_hdu: Some(0), ..Default::default() },
        LoadOptions { error_hdu: Some(9), ..Default::default() },
        LoadOptions { error_column: Some(2), ..Default::default() },
        LoadOptions { error_hdu: Some(1), error_column: Some(2), ..Default::default() },
        LoadOptions { error_hdu: Some(1), ignore_errors: true, ..Default::default() }] {
        assert!(load_bytes_with_options(&bytes, "numerical-oracle.fits", options).is_err());
    }
    for (axes, errors) in [(vec![3], vec![1.; 3]), (vec![2, 2], vec![1.; 4])] {
        let wrong = spectrum("ERR", &axes, &errors, &[]);
        assert!(load_bytes_with_options(&wrong, "numerical-oracle.fits", options).is_err());
    }
    assert!(load_bytes_with_options(b"1 2\n2 3\n", "numerical-oracle.txt", options).is_err());
}

#[test]
fn flxunits_only_fills_missing_or_blank_bunit() {
    for (units, expected) in [(vec!["FLXUNITS= 'erg/s/cm2/A'"], "erg/s/cm2/A"),
        (vec!["BUNIT   = '   '", "FLXUNITS= 'erg/s/cm2/A'"], "erg/s/cm2/A"),
        (vec!["BUNIT   = 'adu'", "FLXUNITS= 'erg/s/cm2/A'"], "adu")] {
        let bytes = spectrum("ERR", &[4], &[1.; 4], &units);
        let image = load_bytes_with_options(&bytes, "numerical-oracle.fits", LoadOptions::default()).unwrap();
        assert_eq!(image.info.spectrum.as_ref().unwrap().y_unit, expected);
        assert!(image.spectrum.as_ref().unwrap().error.is_none());
        assert_eq!(image.physical(0), Some(3.));
    }
}

#[test]
fn null_primary_and_nonviewable_table_headers_are_independent_and_gzip_safe() {
    let mut bytes = Vec::new();
    append_image(&mut bytes, true, &[], &[], &["TARGNAME= 'numerical-header-oracle'", "FILTER  = 'R'", "EXPTIME =                   20"]);
    append_image(&mut bytes, false, &[2, 2], &[1., 2., 3., 4.], &["EXTNAME = 'SCI'", "BUNIT   = 'COUNTS/S'"]);
    bytes.extend(header(&["XTENSION= 'TABLE   '".into(), "BITPIX  =                    8".into(), "NAXIS   =                    2".into(),
        "NAXIS1  =                    4".into(), "NAXIS2  =                    1".into(), "PCOUNT  =                    0".into(), "GCOUNT  =                    1".into(),
        "TFIELDS =                    1".into(), "TTYPE1  = 'LABEL'".into(), "TBCOL1  =                    1".into(), "TFORM1  = 'A4'".into(), "EXTNAME = 'CATALOG'".into()]));
    bytes.extend(b"ABCD");
    bytes.resize(bytes.len().div_ceil(2880) * 2880, b' ');
    let mut file = tempfile::Builder::new().suffix(".fits").tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let primary = load_header(file.path(), 0).unwrap();
    let science = load_header(file.path(), 1).unwrap();
    let table = load_header(file.path(), 2).unwrap();
    assert!(primary.contains("TARGNAME") && !primary.contains("BUNIT"));
    assert!(science.contains("COUNTS/S") && !science.contains("TARGNAME"));
    assert!(table.contains("CATALOG") && table.contains("TTYPE1"));
    assert!(load_header(file.path(), 3).is_err());
    let image = load_file(file.path(), None).unwrap();
    assert_eq!(image.info.hdu, 1);
    assert_eq!(image.info.header, science);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&bytes).unwrap();
    let packed = gzip.finish().unwrap();
    let mut gzfile = tempfile::Builder::new().suffix(".fits.gz").tempfile().unwrap();
    gzfile.write_all(&packed).unwrap();
    assert_eq!(load_header(gzfile.path(), 0).unwrap(), primary);
    assert_eq!(load_header(gzfile.path(), 2).unwrap(), table);
}

#[test]
fn header_limit_and_gzip_spectral_input_limits_are_explicit() {
    let mut cards = vec!["SIMPLE  =                    T".into(), "BITPIX  =                    8".into(), "NAXIS   =                    0".into()];
    cards.extend((0..14000).map(|_| "COMMENT numerical header limit oracle".into()));
    let bytes = header(&cards);
    let mut file = tempfile::Builder::new().suffix(".fits").tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    assert!(load_header(file.path(), 0).unwrap_err().contains("1 MiB"));

    let raw = spectrum("IVAR", &[4], &[4., 0., -1., f64::NAN], &[]);
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&raw).unwrap();
    let bytes = gzip.finish().unwrap();
    let options = LoadOptions { error_hdu: Some(1), ..Default::default() };
    let image = load_bytes_with_options(&bytes, "numerical-oracle.fits.gz", options).unwrap();
    same(image.spectrum.unwrap().error.unwrap().as_slice(), &[0.5, f64::NAN, f64::NAN, f64::NAN]);
}

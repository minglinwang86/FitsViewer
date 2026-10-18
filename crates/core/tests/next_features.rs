use fitsviewer_core::*;
use std::io::Write;

#[test]
fn text_spectrum_and_binary_roundtrip_keep_gaps_and_precision() {
    let mut file=tempfile::Builder::new().suffix(".txt").tempfile().unwrap();
    file.write_all(b"# WAVE FLUX IVAR\n5000 1.00000000000001 4\n5001 nan 0\n5002 -2 9\n").unwrap();
    let original=load_file(file.path(),None).unwrap();
    let copy=FitsImage::from_packet(&original.to_packet().unwrap()).unwrap();
    assert_eq!(copy.physical(0),Some(1.00000000000001));
    assert_eq!(copy.physical(1),None);
    assert_eq!(copy.physical(2),Some(-2.));
    let data=copy.spectrum.unwrap();
    assert_eq!(data.x,vec![5000.,5001.,5002.]);
    assert_eq!(data.error.as_ref().unwrap()[0],0.5);
    assert!(data.error.as_ref().unwrap()[1].is_nan());
    assert!(FitsImage::from_packet(&original.to_packet().unwrap()[..30]).is_err());
}

#[test]
fn malformed_text_and_explicit_column_choices_are_bounded() {
    let mut file=tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    file.write_all(b"x,y,sigma\n1,10,0.1\n2,20,0.2\n").unwrap();
    let chosen=load_file_with_options(file.path(),LoadOptions{x_column:Some(1),y_column:Some(0),error_column:Some(2),..Default::default()}).unwrap();
    assert_eq!(chosen.spectrum.as_ref().unwrap().x,vec![10.,20.]);
    assert_eq!(chosen.physical(0),Some(1.));
    assert!(load_file_with_options(file.path(),LoadOptions{y_column:Some(200),..Default::default()}).is_err());
    assert!(load_file_with_options(file.path(),LoadOptions{error_column:Some(200),..Default::default()}).is_err());
    let no_error=load_file_with_options(file.path(),LoadOptions{ignore_errors:true,..Default::default()}).unwrap();
    assert!(no_error.spectrum.unwrap().error.is_none());
}

#[test]
fn missing_csv_values_remain_gaps() {
    let mut file=tempfile::Builder::new().suffix(".csv").tempfile().unwrap();
    file.write_all(b"WAVE,FLUX,ERR\n1,,0.1\n2,20,0.2\n").unwrap();
    let image=load_file(file.path(),None).unwrap();
    assert_eq!(image.spectrum.as_ref().unwrap().x,vec![1.,2.]);
    assert_eq!(image.physical(0),None);
    assert_eq!(image.physical(1),Some(20.));
}

#[test]
fn unsupported_spectral_wcs_is_explicitly_a_sample_axis() {
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/next");
    let mut bytes=std::fs::read(root.join("spectrum_1d.fits")).unwrap();
    let position=bytes.chunks_exact(80).position(|card|card.starts_with(b"CTYPE1  ")).unwrap()*80;
    let card=format!("{:<80}","CTYPE1  = 'WAVE-F2W'");
    bytes[position..position+80].copy_from_slice(card.as_bytes());
    let mut file=tempfile::Builder::new().suffix(".fits").tempfile().unwrap();
    file.write_all(&bytes).unwrap();
    let image=load_file(file.path(),None).unwrap();
    assert_eq!(image.info.spectrum.as_ref().unwrap().x_unit,"pixel");
    assert!(image.info.spectrum.as_ref().unwrap().note.contains("No supported wavelength"));
    assert_eq!(image.spectrum.as_ref().unwrap().x[0],1.);
}

#[test]
fn fits_and_spectrum_match_independent_astropy_references() {
    let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/next");
    let cases:serde_json::Value=serde_json::from_slice(&std::fs::read(root.join("reference.json")).unwrap()).unwrap();
    for case in cases.as_array().unwrap() {
        let name=case["file"].as_str().unwrap();
        let result=load_file(&root.join(name),None);
        if !case["expected_supported"].as_bool().unwrap() {assert!(result.is_err(),"{name}");continue;}
        let image=result.unwrap_or_else(|e|panic!("{name}: {e}"));
        let reference=case["values"].as_array().unwrap();
        assert_eq!(image.pixels.len(),reference.len(),"{name}");
        for (i,value) in reference.iter().enumerate() {
            if let Some(expected)=value.as_f64() {let actual=image.physical(i).unwrap_or_else(||panic!("{name} invalid {i}"));assert!((actual-expected).abs()<=1e-5*expected.abs().max(1.),"{name} pixel {i}: {actual} != {expected}");}
            else {assert!(image.physical(i).is_none(),"{name} pixel {i}");}
        }
        if case["kind"]=="spectrum" {
            let spectrum=image.spectrum.as_ref().unwrap();
            for (i,v) in case["x"].as_array().unwrap().iter().enumerate(){assert!((spectrum.x[i]-v.as_f64().unwrap()).abs()<1e-8,"{name} x {i}");}
            if let Some(errors)=case.get("error").and_then(|v|v.as_array()) {
                for (i,v) in errors.iter().enumerate(){let actual=spectrum.error.as_ref().unwrap()[i];if let Some(expected)=v.as_f64(){assert!((actual-expected).abs()<1e-10,"{name} error {i}");}else{assert!(actual.is_nan());}}
            }
        }
        let copy=FitsImage::from_packet(&image.to_packet().unwrap()).unwrap();
        assert_eq!(copy.pixels.len(),reference.len());
    }
}

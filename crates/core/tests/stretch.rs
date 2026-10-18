use fitsviewer_core::{*,display::*};

fn image(values:Vec<f64>)->FitsImage{FitsImage{info:ImageInfo{path:"synthetic".into(),hdu:0,width:values.len(),height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,header:String::new(),hdus:vec![],spectrum:None},pixels:RawPixels::F64(values),spectrum:None}}

#[test]
fn eight_stretches_keep_endpoints_and_monotonicity(){
    assert_eq!(Stretch::ALL.len(),8);
    for stretch in Stretch::ALL{for exponent in [1.000001,100.,1000.,1e6]{
        let mapping=ScaleMapping::new(None,0.,1.,stretch,exponent);
        assert_eq!(mapping.apply(-10.),0.);assert!((mapping.apply(10.)-1.).abs()<1e-14);
        let mut previous=0.;for i in 0..=1000{let v=mapping.apply(i as f64/1000.);assert!(v.is_finite() && v>=previous-1e-14 && v>=0. && v<=1.+1e-14,"{}",stretch.name());previous=v;}
    }}
    assert_eq!(Stretch::parse("pow"),Some(Stretch::Power));assert_eq!(Stretch::parse("histequ"),Some(Stretch::HistogramEqualization));
    assert_eq!(Stretch::Squared.apply(0.5),0.25);
    assert!((Stretch::Power.apply(0.5)-(1000f64.sqrt()-1.)/999.).abs()<1e-14);
    assert!((Stretch::Sinh.apply(0.5)-1.5f64.sinh()/3f64.sinh()).abs()<1e-14);
}

#[test]
fn histogram_uses_clipped_finite_pixels_ties_and_shared_color_mapping(){
    let im=image(vec![-9.,0.,0.,0.25,0.25,0.5,1.,99.,f64::NAN]);
    let mapping=ScaleMapping::new(Some(&im),0.,1.,Stretch::HistogramEqualization,1000.);
    for (x,y) in [(0.,0.),(0.1,0.),(0.25,0.5),(0.5,0.75),(1.,1.)]{assert!((mapping.apply(x)-y).abs()<1e-14);}
    let options=RenderOptions::default();let tone=DisplayTone::default();
    let pixels=render_with_mapping(&im,false,tone,&options,&mapping);
    assert_eq!(&pixels[3*4..4*4],&mapping.color(0.25,false,tone,&options.palette));
    assert_eq!(&pixels[8*4..9*4],&[95,45,110,255]);
    assert_eq!(pixels,render_rgba(&im,0.,1.,Stretch::HistogramEqualization,false));
    assert!(mapping.matches(0.,1.,Stretch::HistogramEqualization,1000.));
    assert!(!mapping.matches(0.,2.,Stretch::HistogramEqualization,1000.));
    for values in [vec![0.5;5],vec![f64::NAN;5],vec![2.;5]]{let im=image(values);let fallback=ScaleMapping::new(Some(&im),0.,1.,Stretch::HistogramEqualization,1000.);assert_eq!(fallback.apply(0.3),0.3);}
}

#[test]
fn view_roundtrip_retains_all_stretches_and_validates_exponent(){
    for stretch in Stretch::ALL{let mut view=view::View::default();view.stretch=stretch.name().into();view.render.stretch_exponent=100.;let value=view::View::parse(&serde_json::to_string(&view).unwrap()).unwrap();assert_eq!(value.stretch,stretch.name());assert_eq!(value.render.stretch_exponent,100.);}
    assert_eq!(view::View::parse("{}").unwrap().render.stretch_exponent,1000.);
    for exponent in [f64::NAN,f64::INFINITY,0.,1.,1e7]{let options=RenderOptions{stretch_exponent:exponent,..RenderOptions::default()};assert!(options.validate().is_err());}
}

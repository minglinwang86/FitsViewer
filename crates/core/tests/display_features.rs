use fitsviewer_core::{*,display::*};

fn image()->FitsImage {FitsImage{info:ImageInfo{path:"synthetic".into(),hdu:0,width:2,height:2,bitpix:16,bscale:2.,bzero:10.,blank:Some(-999),header:String::new(),hdus:vec![],spectrum:None},pixels:RawPixels::I16(vec![0,1,2,-999]),spectrum:None}}
#[test]
fn histogram_uses_physical_values_and_ignores_blank(){
    let hist=Histogram::new(&image(),8);
    assert_eq!(hist.finite,3);assert_eq!(hist.minimum,10.);assert_eq!(hist.maximum,14.);assert_eq!(hist.counts.iter().sum::<u64>(),3);
}
#[test]
fn colorbar_and_image_share_the_actual_tone_and_palette(){
    let im=image();let opts=RenderOptions{palette:Palette::builtin("Heat"),mask:None,..RenderOptions::default()};let tone=DisplayTone{bias:0.3,contrast:1.7};
    let output=render(&im,10.,14.,Stretch::Log,true,tone,&opts);
    assert_eq!(&output[..4],&display_color(14.,10.,14.,Stretch::Log,true,tone,&opts.palette));
    assert_eq!(&output[4..8],&[95,45,110,255]);
    assert_eq!(&output[8..12],&display_color(10.,10.,14.,Stretch::Log,true,tone,&opts.palette));
}
#[test]
fn threshold_is_in_scaled_units_and_does_not_modify_source(){
    let im=image();let opts=RenderOptions{palette:Palette::default(),mask:Some(ThresholdMask{threshold:13.,opacity:1.,color:[255,0,0],style:MaskStyle::Fill}),..RenderOptions::default()};
    let output=render(&im,10.,14.,Stretch::Linear,false,DisplayTone::default(),&opts);
    assert_eq!(&output[..4],&[255,0,0,255]);assert_eq!(im.physical(2),Some(14.));assert_eq!(im.pixels.raw_string(2),"2");
    assert!(Palette{name:"broken".into(),colors:vec![]}.validate().is_err());
}

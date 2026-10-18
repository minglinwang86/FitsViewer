use fitsviewer_core::{*,regions::*};
fn image(value:f64)->FitsImage {FitsImage{info:ImageInfo{path:"synthetic".into(),hdu:0,width:64,height:64,bitpix:-64,bscale:1.,bzero:0.,blank:None,header:String::new(),hdus:vec![],spectrum:None},pixels:RawPixels::F64(vec![value;4096]),spectrum:None}}
fn circle()->Aperture {Aperture{name:"source".into(),shape:Shape::Circle{center:[31.3,32.7],radius:5.2},background:Some(Shape::Ring{center:[31.3,32.7],inner:8.,outer:12.}),exclude:vec![]}}
#[test]
fn fractional_coverage_and_constant_background_are_consistent(){
 let result=measure(&image(37.),&circle(),&MeasureOptions::default(),None).unwrap();
 assert!((result.effective_area-std::f64::consts::PI*5.2_f64.powi(2)).abs()<0.08);
 assert!((result.net_flux).abs()<1e-8);assert!((result.background_per_pixel.unwrap()-37.).abs()<1e-10);
 assert!(result.total_error.is_none());assert!(result.snr.is_none());
}
#[test]
fn negative_flux_blanks_and_explicit_error_map_are_preserved(){
 let mut im=image(10.);if let RawPixels::F64(v)=&mut im.pixels{for y in 28..36{for x in 27..35{v[y*64+x]=-2.;}}v[31*64+31]=f64::NAN;}
 let errors=vec![2.;4096];let result=measure(&im,&circle(),&MeasureOptions::default(),Some(&errors)).unwrap();
 assert!(result.net_flux<0.);assert!(result.snr.unwrap()<0.);assert!(result.flags.iter().any(|f|f.contains("Invalid")));
 assert!(result.total_error.unwrap()>0.);
}
#[test]
fn line_union_and_pixel_template_translation_do_not_duplicate_flux(){
 let im=image(2.);let shape=Shape::Path{points:vec![[20.,20.],[30.,20.],[30.,30.]],width:4.,inner_width:0.,smooth:false};
 let aperture=Aperture{name:"line".into(),shape:shape.clone(),background:None,exclude:vec![]};
 let result=measure(&im,&aperture,&MeasureOptions{clip_sigma:3.,subpixels:8,..Default::default()},None).unwrap();
 assert!((result.aperture_sum-2.*result.effective_area).abs()<1e-10);
 let moved=shape.translated(4.5,-2.);
 assert!(moved.contains([25.,18.]));assert!(!moved.contains([25.,25.]));
 let mut edge=circle();edge.shape=Shape::Circle{center:[0.,0.],radius:5.};edge.background=None;
 assert!(measure(&im,&edge,&MeasureOptions::default(),None).unwrap().flags.iter().any(|f|f.contains("boundary")));
}
#[test]
fn background_error_propagates_even_when_measured_background_is_constant(){
 let im=image(10.);let errors=vec![2.;4096];
 let aperture=Aperture{name:"boxes".into(),shape:Shape::Rectangle{center:[20.,20.],size:[3.,3.],angle_deg:0.},background:Some(Shape::Rectangle{center:[30.,30.],size:[3.,3.],angle_deg:0.}),exclude:vec![]};
 let result=measure(&im,&aperture,&MeasureOptions::default(),Some(&errors)).unwrap();
 assert_eq!(result.effective_area,9.);assert_eq!(result.background_area,9.);
 assert!((result.total_error.unwrap().powi(2)-72.).abs()<1e-9);
 let mut contaminated=im;if let RawPixels::F64(v)=&mut contaminated.pixels{v[30*64+30]=1e6;}
 let result=measure(&contaminated,&aperture,&MeasureOptions::default(),Some(&errors)).unwrap();
 assert_eq!(result.background_area,8.);assert!((result.net_flux).abs()<1e-9);
 assert!((result.total_error.unwrap().powi(2)-76.5).abs()<1e-9);
}
#[test]
fn nested_polygons_and_large_geometry_are_bounded(){
 let polygon=Shape::Polygon{points:vec![[10.,10.],[30.,10.],[30.,30.],[10.,30.]],holes:vec![vec![[15.,15.],[25.,15.],[25.,25.],[15.,25.]]]};
 assert!(polygon.contains([12.,12.]));assert!(!polygon.contains([20.,20.]));
 let mut aperture=circle();aperture.shape=polygon;aperture.background=None;
 let result=measure(&image(2.),&aperture,&MeasureOptions::default(),None).unwrap();assert!((result.effective_area-300.).abs()<1e-9);
 aperture.shape=Shape::Polygon{points:(0..2048).map(|i|{let a=i as f64/2048.*std::f64::consts::TAU;[31.+30.*a.cos(),31.+30.*a.sin()]}).collect(),holes:vec![]};
 assert!(measure(&image(2.),&aperture,&MeasureOptions::default(),None).is_err());
}
#[test]
fn shared_source_background_pixels_include_covariance(){
 let im=image(10.);let errors=vec![2.;4096];
 // Disjoint half-pixel source/background regions use the same noisy pixel.
 // 几何不重叠的左右半像素源区与背景区，仍共享同一个原始像素噪声。
 let aperture=Aperture{name:"halves".into(),shape:Shape::Rectangle{center:[19.75,20.],size:[0.5,1.],angle_deg:0.},background:Some(Shape::Rectangle{center:[20.25,20.],size:[0.5,1.],angle_deg:0.}),exclude:vec![]};
 let result=measure(&im,&aperture,&MeasureOptions::default(),Some(&errors)).unwrap();
 assert_eq!(result.effective_area,0.5);assert_eq!(result.background_area,0.5);
 assert!(result.net_flux.abs()<1e-10);assert!(result.total_error.unwrap()<1e-10);
 // Moving the background to the next pixel makes the two noises independent.
 let mut independent=aperture;independent.background=Some(Shape::Rectangle{center:[20.75,20.],size:[0.5,1.],angle_deg:0.});
 let result=measure(&im,&independent,&MeasureOptions::default(),Some(&errors)).unwrap();
 assert!((result.total_error.unwrap().powi(2)-2.).abs()<1e-9);
}
#[test]
fn growth_curve_keeps_the_explicit_error_map(){
 let im=image(10.);let errors=vec![2.;4096];let aperture=circle();let options=MeasureOptions::default();
 let radii=[1.,2.,3.];let curve=growth_curve_with_errors(&im,&aperture,&options,&radii,Some(&errors)).unwrap();
 for (radius,result) in curve{let mut expected=aperture.clone();expected.shape=Shape::Circle{center:[31.3,32.7],radius};let direct=measure(&im,&expected,&options,Some(&errors)).unwrap();assert!(result.total_error.unwrap()>0.);assert!((result.total_error.unwrap()-direct.total_error.unwrap()).abs()<1e-12);}
}

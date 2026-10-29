//! Shared locale dictionaries, parsed only when selected; scientific values stay untouched.
//! native/WASM 与浏览器共用词典；英语及未知科学标识直接返回输入。
use std::{cell::Cell, collections::HashMap, ops::Deref, sync::OnceLock};
use serde::Deserialize;

#[derive(Deserialize)]
struct Locale { code:String, name:String, label:String, file:String }
#[derive(Deserialize)]
struct Registry { schema:u8, default:String, languages:Vec<Locale> }
const SOURCES:&[(&str,&str)]=&[
    ("en.json",include_str!("../../../web/locales/en.json")),
    ("zh-CN.json",include_str!("../../../web/locales/zh-CN.json")),
    ("it.json",include_str!("../../../web/locales/it.json")),
    ("es.json",include_str!("../../../web/locales/es.json")),
    ("de.json",include_str!("../../../web/locales/de.json")),
    ("fr.json",include_str!("../../../web/locales/fr.json")),
];
static REGISTRY:OnceLock<Registry>=OnceLock::new();
static DICTIONARIES:OnceLock<Vec<OnceLock<HashMap<String,String>>>>=OnceLock::new();
static LABELS:OnceLock<Vec<(&'static str,&'static str)>>=OnceLock::new();
fn registry()->&'static Registry {
    REGISTRY.get_or_init(||{
        let registry:Registry=serde_json::from_str(include_str!("../../../web/locales/registry.json")).expect("Valid locale registry");
        assert_eq!(registry.schema,1);assert_eq!(registry.default,"en");
        assert_eq!(registry.languages.len(),SOURCES.len());
        assert_eq!(registry.languages[0].code,"en");
        for locale in &registry.languages{assert_eq!(locale.file,format!("{}.json",locale.code));assert!(SOURCES.iter().any(|(file,_)|*file==locale.file));}
        registry
    })
}
fn labels()->&'static [(&'static str,&'static str)] {
    LABELS.get_or_init(||registry().languages.iter().map(|locale|(locale.code.as_str(),locale.name.as_str())).collect())
}
/// Iterable compatibility facade; names and codes have one canonical JSON source.
/// 保留现有 LANGUAGES 循环/切片接口；默认英语不解析任何大词典。
#[derive(Clone,Copy)]
pub struct LanguageRegistry;
pub const LANGUAGES:LanguageRegistry=LanguageRegistry;
impl Deref for LanguageRegistry {
    type Target=[(&'static str,&'static str)];
    fn deref(&self)->&Self::Target{labels()}
}
impl IntoIterator for LanguageRegistry {
    type Item=(&'static str,&'static str);
    type IntoIter=std::iter::Copied<std::slice::Iter<'static,Self::Item>>;
    fn into_iter(self)->Self::IntoIter{labels().iter().copied()}
}
impl IntoIterator for &LanguageRegistry {
    type Item=&'static (&'static str,&'static str);
    type IntoIter=std::slice::Iter<'static,(&'static str,&'static str)>;
    fn into_iter(self)->Self::IntoIter{labels().iter()}
}
pub fn supported(language:&str)->bool{LANGUAGES.iter().any(|(code,_)|*code==language)}
pub fn language_label(language:&str)->&'static str{
    let registry=registry();registry.languages.iter().find(|locale|locale.code==language).unwrap_or(&registry.languages[0]).label.as_str()
}
thread_local!{static LANGUAGE:Cell<usize>=const{Cell::new(0)};}
pub fn set(language:&str)->bool{
    let next=registry().languages.iter().position(|locale|locale.code==language).unwrap_or(0);
    LANGUAGE.with(|value|{let changed=value.get()!=next;value.set(next);changed})
}
pub fn t(text:&str)->&str{
    let index=LANGUAGE.with(Cell::get);
    if index==0{return text;}
    let dictionaries=DICTIONARIES.get_or_init(||registry().languages.iter().map(|_|OnceLock::new()).collect());
    let source=SOURCES.iter().find(|(file,_)|*file==registry().languages[index].file).expect("Registered locale source").1;
    dictionaries[index].get_or_init(||serde_json::from_str(source).expect("Valid locale dictionary"))
        .get(text).map(String::as_str).unwrap_or(text)
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test] fn six_languages_switch_without_allocating_unknown_values(){
        for (language,_) in LANGUAGES{
            set(language);let value=String::from("FLUX_ERR / C:\\data\\étoile.fits / constructor");
            assert!(std::ptr::eq(t(&value),value.as_str()));
        }
        for (language,open) in [("es","Abrir"),("de","Öffnen"),("fr","Ouvrir")]{set(language);assert_eq!(t("Open"),open);}
        set("en");
    }
    #[test] fn language_switch_preserves_data(){set("en");assert_eq!(t("Open"),"Open");set("zh-CN");assert_eq!(t("Open"),"打开");assert_eq!(t("FLUX_ERR"),"FLUX_ERR");assert_eq!(t("中文路径.fits"),"中文路径.fits");set("en");assert_eq!(t("Aperture photometry"),"Aperture photometry");}
    #[test] fn italian_switches_directly_from_both_existing_languages(){
        for previous in ["en","zh-CN"]{set(previous);assert!(set("it"));assert!(!set("it"));assert_eq!(t("Open"),"Apri");assert_eq!(t("Spectral analysis"),"Analisi spettrale");assert_eq!(t("Aperture photometry"),"Fotometria di apertura");assert!(set(previous));}
        set("it");assert!(set("unsupported"));assert_eq!(t("Open"),"Open");
    }
    #[test] fn all_languages_preserve_unknown_scientific_values_and_paths(){
        for (language,_) in LANGUAGES{assert!(supported(language));set(language);
            for value in ["FLUX_ERR","1.234e-16","RA---TAN","Hα","C:\\dati\\è una stella.fits","中文路径.fits","User palette Ω","unknown service error"]{assert_eq!(t(value),value);}
        }
        assert!(!supported("it-IT"));assert!(!supported("unknown"));set("en");
    }
    #[test] fn italian_retains_scientific_limits_and_uncertainty_boundaries(){
        set("it");assert!(t("Use 16-bit display tiles for large images in Minimal mode").contains("Minimale"));
        assert!(t("Smoothing requires an odd 3–1001 sample window and valid parameters").contains("3–1001"));
        assert!(t("Total uncertainty not computed: provide an error map or explicit electron gain/read noise").starts_with("Incertezza totale non calcolata"));
        assert!(t("Error HDU (zero-based): ").ends_with(": "));set("en");
    }
}

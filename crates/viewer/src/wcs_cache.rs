//! Exact, bounded cursor samples from the external Astropy WCS implementation.
//! 只缓存外部 Astropy 的精确像素结果；不插值，不创建整幅坐标数组。
use serde_json::{json, Value};
use std::collections::VecDeque;

const MAX_POINTS: usize = 512;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_POINT_BYTES: usize = 16 * 1024;

#[derive(PartialEq)]
struct Binding {
    generation: u64,
    path: String,
    hdu: usize,
    header: Option<String>,
}

struct Entry {
    pixel: [f64; 2],
    value: Value,
    bytes: usize,
}

struct Pending {
    label: String,
    pixel: [f64; 2],
}

#[derive(Default)]
pub struct PixelCache {
    binding: Option<Binding>,
    entries: VecDeque<Entry>,
    bytes: usize,
    pending: Option<Pending>,
    serial: u64,
}

impl PixelCache {
    /// A model/document change invalidates every sample and outstanding reply.
    /// 文档、HDU 或完整 WCS 头改变时，清空旧像素与待收结果。
    pub fn bind(&mut self, generation: u64, path: &str, hdu: usize, header: Option<&str>) -> bool {
        if self.binding.as_ref().is_some_and(|old| {
            old.generation == generation && old.path == path && old.hdu == hdu
                && old.header.as_deref() == header
        }) {
            return false;
        }
        self.clear();
        self.binding = Some(Binding {
            generation, path: path.into(), hdu, header: header.map(str::to_owned),
        });
        true
    }

    pub fn clear(&mut self) {
        self.binding = None;
        self.entries.clear();
        self.bytes = 0;
        self.pending = None;
        // Keep the serial monotonic even across close/rebind. / 关闭后也不复用请求编号。
    }

    pub fn get(&mut self, pixel: [f64; 2]) -> Option<Value> {
        if !pixel.iter().all(|v| v.is_finite()) { return None; }
        let index = self.entries.iter().position(|entry| entry.pixel == pixel)?;
        let entry = self.entries.remove(index)?;
        let value = entry.value.clone();
        self.entries.push_back(entry);
        Some(value)
    }

    pub fn is_pending(&self) -> bool { self.pending.is_some() }

    pub fn pending_pixel(&self, label: &str) -> Option<[f64; 2]> {
        self.pending.as_ref().filter(|pending| pending.label == label).map(|pending| pending.pixel)
    }

    /// At most one worker request is in flight; cache hits remain usable meanwhile.
    /// 只保留一个在途请求；等待期间仍可立即读取已缓存像素。
    pub fn begin(&mut self, pixel: [f64; 2]) -> Option<String> {
        if self.binding.is_none() || self.pending.is_some() || !pixel.iter().all(|v| v.is_finite()) {
            return None;
        }
        self.serial = self.serial.checked_add(1)?;
        let label = format!("wcs.{}", self.serial);
        self.pending = Some(Pending { label: label.clone(), pixel });
        Some(label)
    }

    pub fn fail(&mut self, label: &str) -> bool {
        if self.pending_pixel(label).is_none() { return false; }
        self.pending = None;
        true
    }

    /// A late reply cannot clear another request or populate a different model.
    /// 迟到结果不能结束新请求，也不能写入其他文档或坐标模型。
    pub fn accept(&mut self, label: &str, value: Value) -> Result<Option<[f64; 2]>, String> {
        let Some(pixel) = self.pending_pixel(label) else { return Ok(None); };
        self.pending = None;
        if point_at(&value, pixel).is_none() {
            return Err("J2000 response does not match the requested pixel or coordinate frame".into());
        }
        let bytes = serde_json::to_vec(&value).map_err(|error| error.to_string())?.len();
        if bytes > MAX_POINT_BYTES {
            return Err("J2000 point response exceeds the cache limit".into());
        }
        if let Some(index) = self.entries.iter().position(|entry| entry.pixel == pixel) {
            if let Some(old) = self.entries.remove(index) { self.bytes -= old.bytes; }
        }
        self.bytes += bytes;
        self.entries.push_back(Entry { pixel, value, bytes });
        while self.entries.len() > MAX_POINTS || self.bytes > MAX_BYTES {
            if let Some(old) = self.entries.pop_front() { self.bytes -= old.bytes; }
        }
        Ok(Some(pixel))
    }

    pub fn summary(&self) -> Value {
        json!({"points":self.entries.len(),"bytes":self.bytes,"max_points":MAX_POINTS,
               "max_bytes":MAX_BYTES,"pending":self.pending.is_some(),"exact_pixels":true,
               "generation":self.binding.as_ref().map(|binding|binding.generation),
               "hdu":self.binding.as_ref().map(|binding|binding.hdu)})
    }
}

/// Only an exact zero-based FK5/J2000 sample can label the current cursor.
/// 仅当像素、原点和坐标系完全匹配时，才可给当前光标显示 J2000。
pub fn point_at(value: &Value, pixel: [f64; 2]) -> Option<&Value> {
    if !pixel.iter().all(|v| v.is_finite()) || value["frame"] != "FK5"
        || value["equinox"] != "J2000" || value["coordinate_origin"].as_u64() != Some(0)
    { return None; }
    let points = value["points"].as_array()?;
    if points.len() != 1 { return None; }
    let point = points.first()?;
    let actual = point["pixel"].as_array()?;
    if actual.len() != 2 || actual[0].as_f64()? != pixel[0] || actual[1].as_f64()? != pixel[1] {
        return None;
    }
    let (ra, dec) = (point["ra_deg"].as_f64()?, point["dec_deg"].as_f64()?);
    if !ra.is_finite() || !dec.is_finite() || !(0.0..360.0).contains(&ra) || !(-90.0..=90.0).contains(&dec) {
        return None;
    }
    point["sexagesimal"].as_str().filter(|text| !text.trim().is_empty())?;
    Some(point)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pixel: [f64; 2]) -> Value {
        json!({"frame":"FK5","equinox":"J2000","coordinate_origin":0,
               "points":[{"pixel":pixel,"ra_deg":359.999,"dec_deg":-45.,"sexagesimal":"23h59m59.76s -45d00m00s"}]})
    }

    fn cache() -> PixelCache {
        let mut cache = PixelCache::default();
        cache.bind(1, "field.fits", 0, Some("TAN model"));
        cache
    }

    #[test]
    fn same_pixel_returns_exact_sample_and_pending_remains_owned() {
        let mut cache = cache();
        let label = cache.begin([0., 0.]).unwrap();
        cache.accept(&label, sample([0., 0.])).unwrap();
        let next = cache.begin([1., 0.]).unwrap();
        assert_eq!(cache.get([0., 0.]), Some(sample([0., 0.])));
        assert_eq!(cache.get([0.0001, 0.]), None);
        assert!(cache.accept(&label, sample([0., 0.])).unwrap().is_none());
        assert_eq!(cache.pending_pixel(&next), Some([1., 0.]));
    }

    #[test]
    fn model_document_hdu_and_close_invalidate_late_replies() {
        let mut cache = cache();
        let old = cache.begin([1., 2.]).unwrap();
        assert!(!cache.bind(1, "field.fits", 0, Some("TAN model")));
        assert!(cache.bind(1, "field.fits", 1, Some("TAN model")));
        assert!(cache.accept(&old, sample([1., 2.])).unwrap().is_none());
        let label = cache.begin([1., 2.]).unwrap();
        cache.accept(&label, sample([1., 2.])).unwrap();
        assert!(cache.bind(1, "field.fits", 1, None));
        assert!(cache.get([1., 2.]).is_none());
        assert!(cache.bind(2, "field.fits", 1, None));
        let old = cache.begin([1., 2.]).unwrap();
        cache.clear();
        assert!(cache.accept(&old, sample([1., 2.])).unwrap().is_none());
        assert!(cache.begin([1., 2.]).is_none());
    }

    #[test]
    fn lru_is_bounded_and_hot_pixel_survives() {
        let mut cache = cache();
        for x in 0..MAX_POINTS {
            let pixel = [x as f64, 0.];
            let label = cache.begin(pixel).unwrap();
            cache.accept(&label, sample(pixel)).unwrap();
        }
        assert!(cache.get([0., 0.]).is_some());
        let label = cache.begin([1000., 0.]).unwrap();
        cache.accept(&label, sample([1000., 0.])).unwrap();
        assert!(cache.get([0., 0.]).is_some());
        assert!(cache.get([1., 0.]).is_none());
        assert_eq!(cache.summary()["points"], MAX_POINTS);
    }

    #[test]
    fn byte_budget_and_per_point_limits_bound_large_metadata() {
        let mut cache = cache();
        for x in 0..200 {
            let pixel = [x as f64, 0.];
            let mut value = sample(pixel); value["source_frame"] = json!("x".repeat(8000));
            let label = cache.begin(pixel).unwrap();
            cache.accept(&label, value).unwrap();
        }
        assert!(cache.summary()["bytes"].as_u64().unwrap() <= MAX_BYTES as u64);
        assert!(cache.summary()["points"].as_u64().unwrap() < MAX_POINTS as u64);
        assert!(cache.get([0., 0.]).is_none());
        assert!(cache.get([199., 0.]).is_some());
        let label = cache.begin([999., 0.]).unwrap();
        let mut too_large = sample([999., 0.]); too_large["source_frame"] = json!("x".repeat(MAX_POINT_BYTES));
        assert!(cache.accept(&label, too_large).is_err());
        assert!(cache.get([999., 0.]).is_none());
    }

    #[test]
    fn wrong_pixel_frame_origin_and_nonfinite_output_are_rejected() {
        assert!(point_at(&sample([1., 0.]), [0., 0.]).is_none());
        let mut invalid = sample([0., 0.]); invalid["frame"] = json!("ICRS");
        assert!(point_at(&invalid, [0., 0.]).is_none());
        let mut invalid = sample([0., 0.]); invalid["coordinate_origin"] = json!(1);
        assert!(point_at(&invalid, [0., 0.]).is_none());
        let mut invalid = sample([0., 0.]); invalid["points"][0]["ra_deg"] = Value::Null;
        assert!(point_at(&invalid, [0., 0.]).is_none());
        let mut cache = cache(); let label = cache.begin([0., 0.]).unwrap();
        assert!(cache.accept(&label, invalid).is_err());
        assert!(!cache.is_pending()); assert!(cache.get([0., 0.]).is_none());
    }
}

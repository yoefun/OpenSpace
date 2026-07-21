//! Structural wall detection via dark-pixel mask + row/column projection.
//! Suited for colored CAD floor plans (black walls, furniture, room fills).

use crate::Seg;
use image::{DynamicImage, GrayImage};

/// Extract thick dark wall lines from a colored architectural floor plan.
pub fn detect_structural_segments(img: &DynamicImage) -> Vec<Seg> {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    if w < 16 || h < 16 {
        return Vec::new();
    }

    let mut mask = build_structural_mask(img, &gray);
    // Remove thin furniture strokes, keep thick wall strokes.
    let open_r = ((w.min(h) as f32) * 0.002).round() as i32;
    let open_r = open_r.clamp(1, 3);
    crate::erode(&mut mask, open_r);
    crate::dilate(&mut mask, open_r);

    // Directional close: connect wall runs without blobbing furniture.
    let hr = ((w as f32) * 0.012).round() as i32;
    let vr = ((h as f32) * 0.012).round() as i32;
    dilate_horizontal(&mut mask, hr.clamp(3, 18));
    dilate_vertical(&mut mask, vr.clamp(3, 18));
    erode_horizontal(&mut mask, hr.clamp(3, 18));
    erode_vertical(&mut mask, vr.clamp(3, 18));

    remove_small_blobs(&mut mask, ((w * h) / 1500).max(120));
    remove_compact_blobs(&mut mask, ((w.min(h) / 40).max(4)) as u32);

    let min_span_h = w as f32 * 0.16;
    let min_span_v = h as f32 * 0.16;
    let band = (h as f32 * 0.006).max(2.0) as u32;

    let mut segs = Vec::new();
    segs.extend(projection_horizontal(&mask, min_span_h, band));
    segs.extend(projection_vertical(&mask, min_span_v, band));

    let merge_tol = (w.min(h) as f32 * 0.012).clamp(6.0, 18.0);
    let mut segs = crate::dedupe_segments(&segs, merge_tol * 0.9);
    crate::merge_collinear_segments(&mut segs, merge_tol, merge_tol * 2.0);

    let max_len = w.max(h) as f32;
    let min_len = max_len * 0.10;
    segs.retain(|s| s.length() >= min_len);
    score_and_prune_walls(&mut segs, w, h, 56);
    segs
}

fn build_structural_mask(img: &DynamicImage, gray: &GrayImage) -> GrayImage {
    let (w, h) = gray.dimensions();
    let thresh = estimate_wall_threshold(gray).min(72);
    let rgb = img.to_rgb8();
    let mut out = GrayImage::new(w, h);
    for (x, y, p) in gray.enumerate_pixels() {
        let lum = p[0];
        let px = rgb.get_pixel(x, y);
        let r = px[0];
        let g = px[1];
        let b = px[2];
        let maxc = r.max(g).max(b);
        let minc = r.min(g).min(b);
        let neutral = maxc.saturating_sub(minc) < 40;
        let dark = lum < thresh && maxc < thresh.saturating_add(8);
        let v = if dark && neutral { 255u8 } else { 0 };
        out.put_pixel(x, y, image::Luma([v]));
    }
    out
}

fn estimate_wall_threshold(gray: &GrayImage) -> u8 {
    let mut dark: Vec<u8> = gray.pixels().map(|p| p[0]).filter(|&v| v < 100).collect();
    if dark.is_empty() {
        return 65;
    }
    dark.sort_unstable();
    let p85 = dark[dark.len() * 85 / 100];
    (p85 as u16 + 10).min(80) as u8
}

fn dilate_horizontal(mask: &mut GrayImage, rx: i32) {
    if rx <= 0 {
        return;
    }
    let (w, h) = mask.dimensions();
    let copy = mask.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut maxv = 0u8;
            for dx in -rx..=rx {
                let nx = x + dx;
                if nx >= 0 && nx < w as i32 {
                    maxv = maxv.max(copy.get_pixel(nx as u32, y as u32)[0]);
                }
            }
            mask.put_pixel(x as u32, y as u32, image::Luma([maxv]));
        }
    }
}

fn dilate_vertical(mask: &mut GrayImage, ry: i32) {
    if ry <= 0 {
        return;
    }
    let (w, h) = mask.dimensions();
    let copy = mask.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut maxv = 0u8;
            for dy in -ry..=ry {
                let ny = y + dy;
                if ny >= 0 && ny < h as i32 {
                    maxv = maxv.max(copy.get_pixel(x as u32, ny as u32)[0]);
                }
            }
            mask.put_pixel(x as u32, y as u32, image::Luma([maxv]));
        }
    }
}

fn erode_horizontal(mask: &mut GrayImage, rx: i32) {
    if rx <= 0 {
        return;
    }
    let (w, h) = mask.dimensions();
    let copy = mask.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut minv = 255u8;
            for dx in -rx..=rx {
                let nx = x + dx;
                if nx >= 0 && nx < w as i32 {
                    minv = minv.min(copy.get_pixel(nx as u32, y as u32)[0]);
                }
            }
            mask.put_pixel(x as u32, y as u32, image::Luma([minv]));
        }
    }
}

fn erode_vertical(mask: &mut GrayImage, ry: i32) {
    if ry <= 0 {
        return;
    }
    let (w, h) = mask.dimensions();
    let copy = mask.clone();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            let mut minv = 255u8;
            for dy in -ry..=ry {
                let ny = y + dy;
                if ny >= 0 && ny < h as i32 {
                    minv = minv.min(copy.get_pixel(x as u32, ny as u32)[0]);
                }
            }
            mask.put_pixel(x as u32, y as u32, image::Luma([minv]));
        }
    }
}

fn remove_small_blobs(mask: &mut GrayImage, min_area: u32) {
    let (w, h) = mask.dimensions();
    let mut labels = vec![0i32; (w * h) as usize];
    let mut areas: Vec<u32> = vec![0];
    let mut bboxes: Vec<(u32, u32, u32, u32)> = vec![(0, 0, 0, 0)];
    let mut next_label = 1i32;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if mask.get_pixel(x, y)[0] == 0 || labels[idx] != 0 {
                continue;
            }
            let mut stack = vec![(x, y)];
            let mut area = 0u32;
            let mut xmin = x;
            let mut xmax = x;
            let mut ymin = y;
            let mut ymax = y;
            labels[idx] = next_label;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                xmin = xmin.min(cx);
                xmax = xmax.max(cx);
                ymin = ymin.min(cy);
                ymax = ymax.max(cy);
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < w && ny < h {
                        let nidx = (ny * w + nx) as usize;
                        if mask.get_pixel(nx, ny)[0] > 0 && labels[nidx] == 0 {
                            labels[nidx] = next_label;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            areas.push(area);
            bboxes.push((xmin, ymin, xmax, ymax));
            next_label += 1;
        }
    }

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let lbl = labels[idx];
            if lbl > 0 && areas[lbl as usize] < min_area {
                mask.put_pixel(x, y, image::Luma([0]));
            }
        }
    }
}

/// Drop compact blobs (furniture symbols) while keeping elongated wall strokes.
fn remove_compact_blobs(mask: &mut GrayImage, min_thickness: u32) {
    let (w, h) = mask.dimensions();
    let mut labels = vec![0i32; (w * h) as usize];
    let mut meta: Vec<(u32, u32, u32)> = vec![(0, 0, 0)];
    let mut next_label = 1i32;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if mask.get_pixel(x, y)[0] == 0 || labels[idx] != 0 {
                continue;
            }
            let mut stack = vec![(x, y)];
            let mut area = 0u32;
            let mut xmin = x;
            let mut xmax = x;
            let mut ymin = y;
            let mut ymax = y;
            labels[idx] = next_label;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                xmin = xmin.min(cx);
                xmax = xmax.max(cx);
                ymin = ymin.min(cy);
                ymax = ymax.max(cy);
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx < w && ny < h {
                        let nidx = (ny * w + nx) as usize;
                        if mask.get_pixel(nx, ny)[0] > 0 && labels[nidx] == 0 {
                            labels[nidx] = next_label;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            let bw = xmax - xmin + 1;
            let bh = ymax - ymin + 1;
            let thick = bw.min(bh);
            meta.push((area, bw.max(bh), thick));
            next_label += 1;
        }
    }

    let min_span = (w.min(h) / 8).max(12);
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let lbl = labels[idx];
            if lbl <= 0 {
                continue;
            }
            let (area, span, thick) = meta[lbl as usize];
            let compact = thick < min_thickness && span < min_span;
            let tiny = area < 60 && span < min_span / 2;
            if compact || tiny {
                mask.put_pixel(x, y, image::Luma([0]));
            }
        }
    }
}

fn projection_horizontal(mask: &GrayImage, min_span: f32, band: u32) -> Vec<Seg> {
    let (w, h) = mask.dimensions();
    let mut proj = vec![0u32; h as usize];
    for y in 0..h {
        for x in 0..w {
            if mask.get_pixel(x, y)[0] > 0 {
                proj[y as usize] += 1;
            }
        }
    }
    let thresh = (w as f32 * 0.20) as u32;
    let peaks = find_projection_peaks(&proj, thresh, w);
    let mut segs = Vec::new();
    for y in peaks {
        for (x0, x1) in runs_in_row_band(mask, y, band, min_span as u32) {
            segs.push(Seg {
                x0: x0 as f32,
                y0: y as f32,
                x1: x1 as f32,
                y1: y as f32,
            });
        }
    }
    segs
}

fn projection_vertical(mask: &GrayImage, min_span: f32, band: u32) -> Vec<Seg> {
    let (w, h) = mask.dimensions();
    let mut proj = vec![0u32; w as usize];
    for x in 0..w {
        for y in 0..h {
            if mask.get_pixel(x, y)[0] > 0 {
                proj[x as usize] += 1;
            }
        }
    }
    let thresh = (h as f32 * 0.20) as u32;
    let peaks = find_projection_peaks(&proj, thresh, h);
    let mut segs = Vec::new();
    for x in peaks {
        for (y0, y1) in runs_in_col_band(mask, x, band, min_span as u32) {
            segs.push(Seg {
                x0: x as f32,
                y0: y0 as f32,
                x1: x as f32,
                y1: y1 as f32,
            });
        }
    }
    segs
}

fn find_projection_peaks(proj: &[u32], thresh: u32, span: u32) -> Vec<u32> {
    let mut peaks = Vec::new();
    let merge_dist = ((span as f32) * 0.015).round() as usize;
    let merge_dist = merge_dist.clamp(4, 14);
    let mut i = 0;
    while i < proj.len() {
        if proj[i] < thresh {
            i += 1;
            continue;
        }
        let mut best_i = i;
        let mut best_v = proj[i];
        while i < proj.len() && proj[i] >= thresh / 2 {
            if proj[i] > best_v {
                best_v = proj[i];
                best_i = i;
            }
            i += 1;
        }
        if best_v >= thresh {
            if let Some(last) = peaks.last() {
                if best_i.abs_diff(*last as usize) <= merge_dist {
                    continue;
                }
            }
            peaks.push(best_i as u32);
        }
    }
    peaks
}

fn runs_in_row_band(mask: &GrayImage, y: u32, band: u32, min_len: u32) -> Vec<(u32, u32)> {
    let (w, h) = mask.dimensions();
    let y0 = y.saturating_sub(band);
    let y1 = (y + band).min(h - 1);
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for yy in y0..=y1 {
        let mut start: Option<u32> = None;
        for x in 0..w {
            if mask.get_pixel(x, yy)[0] > 0 {
                if start.is_none() {
                    start = Some(x);
                }
            } else if let Some(s) = start {
                if x.saturating_sub(s) >= min_len {
                    merge_interval(&mut merged, s, x - 1);
                }
                start = None;
            }
        }
        if let Some(s) = start {
            if w.saturating_sub(s) >= min_len {
                merge_interval(&mut merged, s, w - 1);
            }
        }
    }
    merged
        .into_iter()
        .filter(|(a, b)| b.saturating_sub(*a) >= min_len)
        .collect()
}

fn runs_in_col_band(mask: &GrayImage, x: u32, band: u32, min_len: u32) -> Vec<(u32, u32)> {
    let (w, h) = mask.dimensions();
    if x >= w {
        return Vec::new();
    }
    let x0 = x.saturating_sub(band);
    let x1 = (x + band).min(w - 1);
    let mut merged: Vec<(u32, u32)> = Vec::new();
    for xx in x0..=x1 {
        let mut start: Option<u32> = None;
        for y in 0..h {
            if mask.get_pixel(xx, y)[0] > 0 {
                if start.is_none() {
                    start = Some(y);
                }
            } else if let Some(s) = start {
                if y.saturating_sub(s) >= min_len {
                    merge_interval(&mut merged, s, y - 1);
                }
                start = None;
            }
        }
        if let Some(s) = start {
            if h.saturating_sub(s) >= min_len {
                merge_interval(&mut merged, s, h - 1);
            }
        }
    }
    merged
        .into_iter()
        .filter(|(a, b)| b.saturating_sub(*a) >= min_len)
        .collect()
}

fn merge_interval(merged: &mut Vec<(u32, u32)>, a: u32, b: u32) {
    for (lo, hi) in merged.iter_mut() {
        if a <= *hi + 8 && b + 8 >= *lo {
            *lo = (*lo).min(a);
            *hi = (*hi).max(b);
            return;
        }
    }
    merged.push((a, b));
}

fn segment_importance(seg: &Seg, w: u32, h: u32) -> f32 {
    let len = seg.length();
    let border_tol = (w.min(h) as f32) * 0.07;
    let mut bonus = 0.0_f32;
    for (x, y) in [(seg.x0, seg.y0), (seg.x1, seg.y1)] {
        let d = x.min(y).min((w as f32) - x).min((h as f32) - y);
        if d < border_tol {
            bonus += 0.6;
        }
    }
    len * (1.0 + bonus)
}

fn score_and_prune_walls(segs: &mut Vec<Seg>, w: u32, h: u32, max: usize) {
    if segs.len() <= max {
        return;
    }
    segs.sort_by(|a, b| {
        segment_importance(b, w, h)
            .partial_cmp(&segment_importance(a, w, h))
            .unwrap()
    });
    segs.truncate(max);
}
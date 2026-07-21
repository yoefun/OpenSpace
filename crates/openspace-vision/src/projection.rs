//! Structural wall detection via dark-pixel mask + row/column projection.
//! Suited for colored CAD floor plans (black walls, furniture, room fills).

use crate::Seg;
use image::GrayImage;

/// Extract thick dark wall lines from a colored architectural floor plan.
pub fn detect_structural_segments(gray: &GrayImage) -> Vec<Seg> {
    let (w, h) = gray.dimensions();
    if w < 16 || h < 16 {
        return Vec::new();
    }

    let wall_thresh = estimate_wall_threshold(gray);
    let mut mask = wall_mask(gray, wall_thresh);
    let close_r = ((w.min(h) as f32) * 0.004).round() as i32;
    let close_r = close_r.clamp(2, 6);
    crate::dilate(&mut mask, close_r);
    crate::erode(&mut mask, close_r);
    remove_small_blobs(&mut mask, ((w * h) / 2000).max(80));

    let min_span_h = w as f32 * 0.10;
    let min_span_v = h as f32 * 0.10;
    let band = (h as f32 * 0.008).max(3.0) as u32;

    let mut segs = Vec::new();
    segs.extend(projection_horizontal(&mask, min_span_h, band));
    segs.extend(projection_vertical(&mask, min_span_v, band));

    let merge_tol = (w.min(h) as f32 * 0.01).clamp(5.0, 14.0);
    let mut segs = crate::dedupe_segments(&segs, merge_tol * 0.9);
    crate::merge_collinear_segments(&mut segs, merge_tol, merge_tol * 1.5);

    let max_len = w.max(h) as f32;
    let min_len = max_len * 0.08;
    segs.retain(|s| s.length() >= min_len);
    prune_to_max_walls(&mut segs, 48);
    segs
}

fn estimate_wall_threshold(gray: &GrayImage) -> u8 {
    let mut dark: Vec<u8> = gray.pixels().map(|p| p[0]).filter(|&v| v < 120).collect();
    if dark.is_empty() {
        return 85;
    }
    dark.sort_unstable();
    let p90 = dark[dark.len() * 9 / 10];
    (p90 as u16 + 15).min(95) as u8
}

fn wall_mask(gray: &GrayImage, thresh: u8) -> GrayImage {
    let (w, h) = gray.dimensions();
    let mut out = GrayImage::new(w, h);
    for (x, y, p) in gray.enumerate_pixels() {
        let v = if p[0] < thresh { 255u8 } else { 0 };
        out.put_pixel(x, y, image::Luma([v]));
    }
    out
}

fn remove_small_blobs(mask: &mut GrayImage, min_area: u32) {
    let (w, h) = mask.dimensions();
    let mut labels = vec![0i32; (w * h) as usize];
    let mut areas: Vec<u32> = vec![0];
    let mut next_label = 1i32;

    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            if mask.get_pixel(x, y)[0] == 0 || labels[idx] != 0 {
                continue;
            }
            let mut stack = vec![(x, y)];
            let mut area = 0u32;
            labels[idx] = next_label;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
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
    let thresh = (w as f32 * 0.12) as u32;
    let peaks = find_projection_peaks(&proj, thresh);
    let mut segs = Vec::new();
    for y in peaks {
        if let Some((x0, x1)) = span_in_row_band(mask, y, band) {
            let len = (x1 - x0) as f32;
            if len >= min_span {
                segs.push(Seg {
                    x0: x0 as f32,
                    y0: y as f32,
                    x1: x1 as f32,
                    y1: y as f32,
                });
            }
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
    let thresh = (h as f32 * 0.12) as u32;
    let peaks = find_projection_peaks(&proj, thresh);
    let mut segs = Vec::new();
    for x in peaks {
        if let Some((y0, y1)) = span_in_col_band(mask, x, band) {
            let len = (y1 - y0) as f32;
            if len >= min_span {
                segs.push(Seg {
                    x0: x as f32,
                    y0: y0 as f32,
                    x1: x as f32,
                    y1: y1 as f32,
                });
            }
        }
    }
    segs
}

fn find_projection_peaks(proj: &[u32], thresh: u32) -> Vec<u32> {
    let mut peaks = Vec::new();
    let merge_dist = 6usize;
    let mut i = 0;
    while i < proj.len() {
        if proj[i] < thresh {
            i += 1;
            continue;
        }
        let start = i;
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

fn span_in_row_band(mask: &GrayImage, y: u32, band: u32) -> Option<(u32, u32)> {
    let (w, h) = mask.dimensions();
    let y0 = y.saturating_sub(band);
    let y1 = (y + band).min(h - 1);
    let mut xmin = w;
    let mut xmax = 0u32;
    let mut any = false;
    for yy in y0..=y1 {
        for x in 0..w {
            if mask.get_pixel(x, yy)[0] > 0 {
                any = true;
                xmin = xmin.min(x);
                xmax = xmax.max(x);
            }
        }
    }
    if any && xmax > xmin {
        Some((xmin, xmax))
    } else {
        None
    }
}

fn span_in_col_band(mask: &GrayImage, x: u32, band: u32) -> Option<(u32, u32)> {
    let (w, h) = mask.dimensions();
    if x >= w {
        return None;
    }
    let x0 = x.saturating_sub(band);
    let x1 = (x + band).min(w - 1);
    let mut ymin = h;
    let mut ymax = 0u32;
    let mut any = false;
    for xx in x0..=x1 {
        for y in 0..h {
            if mask.get_pixel(xx, y)[0] > 0 {
                any = true;
                ymin = ymin.min(y);
                ymax = ymax.max(y);
            }
        }
    }
    if any && ymax > ymin {
        Some((ymin, ymax))
    } else {
        None
    }
}

fn prune_to_max_walls(segs: &mut Vec<Seg>, max: usize) {
    if segs.len() <= max {
        return;
    }
    segs.sort_by(|a, b| b.length().partial_cmp(&a.length()).unwrap());
    segs.truncate(max);
}

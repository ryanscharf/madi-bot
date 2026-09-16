use pdfium_render::prelude::*;
use reqwest::Client;

/// Heading text used by different teams' game notes PDFs for their player
/// availability section. Tried longest/most-specific first so teams using
/// the "REPORT" suffix don't get matched on the shorter substring first.
const HEADING_CANDIDATES: &[&str] = &[
    "PLAYER AVAILABILITY REPORT",
    "PLAYER AVAILABILITY",
    "AVAILABILITY REPORT",
    "UNAVAILABILITY",
];

/// Status values and injury/reason words used as anchors to find the table's
/// extent on the rendered page. Doesn't need to be exhaustive — row-widening
/// (below) picks up any other text sharing a matched anchor's row, including
/// words not in this list, so a status keyword alone is enough to anchor a
/// whole row; these reason words just add extra anchors for rows that don't
/// happen to include one of the plain status values.
///
/// Deliberately excludes short generic words like "FOOT", "HAND", "HEAD",
/// "HIP", "BACK", "REST" — they're common substrings of unrelated words
/// (e.g. "FOOT" inside "...FOOTBALLCLUB.COM" in a page footer planted a
/// false anchor there during testing) and row-widening already covers what
/// they were meant to catch.
const KEYWORDS: &[&str] = &[
    "QUESTIONABLE",
    "DOUBTFUL",
    "SUSPENDED",
    "PROBABLE",
    "UNAVAILABLE",
    "AVAILABLE",
    "OUT",
    "LOWER BODY",
    "UPPER BODY",
    "ILLNESS",
    "CONCUSSION",
    "HAMSTRING",
    "SHOULDER",
    "GROIN",
    "ANKLE",
    "THIGH",
    "KNEE",
    "QUAD",
    "CALF",
    "INJURY",
    "INJURED",
];

/// Some teams (e.g. Sporting JAX) split the table into labeled sub-lists
/// side by side rather than one narrow column, so these get a much wider
/// horizontal tolerance than the other keywords when deciding what's "part
/// of the table."
const SUBLIST_LABELS: &[&str] = &["OUT:", "UNAVAILABLE:", "QUESTIONABLE:", "DOUBTFUL:"];

const MAX_HEIGHT_PT: f32 = 400.0;
const ROW_TOLERANCE_PT: f32 = 3.0;
const ROW_WIDEN_X_PT: f32 = 25.0;
// Trailing heading text (e.g. "(AS OF AUG. 22)", "/Injury Report") always
// sits to the right of the matched substring, never the left, so the two
// margins don't need to match — a small left margin avoids pulling in
// unrelated content in an adjacent column to the left (seen with Dallas
// Trinity's two-column layout), while a generous right margin still catches
// trailing banner text (seen with Fort Lauderdale's).
const HEADING_ROW_WIDEN_LEFT_PT: f32 = 20.0;
const HEADING_ROW_WIDEN_RIGHT_PT: f32 = 180.0;
const NARROW_X_TOLERANCE_PT: f32 = 40.0;
const SUBLIST_X_TOLERANCE_PT: f32 = 260.0;
const SUBLIST_COLUMN_X_PT: f32 = 90.0;
const SUBLIST_COLUMN_HEIGHT_PT: f32 = 70.0;
const PAD_PT: f32 = 14.0;
const RENDER_WIDTH_PX: i32 = 1600;
const RENDER_MAX_HEIGHT_PX: i32 = 2400;

/// Creates a fresh `Pdfium` binding. `Pdfium` isn't `Sync` (its trait object
/// has no `Send + Sync` bound even with the `thread_safe` feature), so it
/// can't live in a shared static — instead each call binds its own instance.
/// This is cheap in practice: the dynamic linker caches the loaded library
/// after the first bind, and every call here already runs inside
/// `spawn_blocking` on its own thread.
fn pdfium() -> anyhow::Result<Pdfium> {
    let bindings = Pdfium::bind_to_system_library()
        .map_err(|e| anyhow::anyhow!("failed to bind pdfium library: {}", e))?;
    Ok(Pdfium::new(bindings))
}

pub async fn fetch_pdf_bytes(client: &Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let bytes = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await?
        .bytes()
        .await?;
    Ok(bytes.to_vec())
}

/// Downloads the game notes PDF and returns a cropped PNG screenshot of the
/// player availability table, or `None` if no known heading/table shape was
/// found on any page.
pub async fn availability_screenshot(client: &Client, url: &str) -> anyhow::Result<Option<Vec<u8>>> {
    let bytes = fetch_pdf_bytes(client, url).await?;
    tokio::task::spawn_blocking(move || crop_availability(&bytes)).await?
}

fn crop_availability(pdf_bytes: &[u8]) -> anyhow::Result<Option<Vec<u8>>> {
    let pdfium = pdfium()?;
    let doc = pdfium.load_pdf_from_byte_slice(pdf_bytes, None)?;

    for page in doc.pages().iter() {
        let text = page.text()?;

        for &heading in HEADING_CANDIDATES {
            let heading_search = text.search(heading, &PdfSearchOptions::new())?;
            let Some(heading_match) = heading_search.find_next() else { continue };
            let Some(heading_rect) = union_of(heading_match.iter().map(|s| s.bounds())) else { continue };

            let h_left = heading_rect.left().value;
            let h_right = heading_rect.right().value;
            let h_top = heading_rect.top().value;

            let all_segments: Vec<PdfRect> = text.segments().iter().map(|s| s.bounds()).collect();

            let mut anchors = vec![heading_rect];
            let mut label_anchors: Vec<PdfRect> = Vec::new();
            for &kw in KEYWORDS.iter().chain(SUBLIST_LABELS.iter()) {
                let is_label = SUBLIST_LABELS.contains(&kw);
                let x_tolerance = if is_label { SUBLIST_X_TOLERANCE_PT } else { NARROW_X_TOLERANCE_PT };

                let search = text.search(kw, &PdfSearchOptions::new())?;
                while let Some(m) = search.find_next() {
                    if let Some(r) = union_of(m.iter().map(|s| s.bounds())) {
                        let same_column =
                            r.left().value < h_right + x_tolerance && r.right().value > h_left - x_tolerance;
                        let below_heading = r.top().value <= h_top + 20.0 && r.top().value > h_top - MAX_HEIGHT_PT;
                        if same_column && below_heading {
                            anchors.push(r);
                            if is_label {
                                label_anchors.push(r);
                            }
                        }
                    }
                }
            }

            // Column extension: sub-list labels (e.g. "QUESTIONABLE:") head
            // a vertical list of names below them, not beside them — pull in
            // whatever sits in that same narrow column underneath each
            // label, since those names usually carry no recognizable
            // keyword of their own (row-widening alone can't reach them,
            // it only looks sideways along a matched row).
            for label in &label_anchors {
                for seg in &all_segments {
                    let same_column = seg.left().value < label.right().value + SUBLIST_COLUMN_X_PT
                        && seg.right().value > label.left().value - SUBLIST_COLUMN_X_PT;
                    let below_label = seg.top().value <= label.bottom().value + 2.0
                        && seg.top().value > label.bottom().value - SUBLIST_COLUMN_HEIGHT_PT;
                    if same_column && below_label {
                        anchors.push(*seg);
                    }
                }
            }

            if anchors.len() < 2 {
                // Just the heading with nothing recognizable nearby — try
                // the next heading candidate rather than crop a bare banner.
                continue;
            }

            // Row-widening: pull in any other text sharing an anchor's row,
            // so trailing heading text and unrecognized-word columns aren't
            // clipped just because we don't have a keyword for them.
            // Bound how far row-widening can reach horizontally by the
            // anchors' own combined width (typically ~= the table's full
            // width, since the heading banner usually spans it), not a
            // flat per-anchor margin — otherwise, on pages that lay the
            // table out next to unrelated content (e.g. a schedule table
            // in an adjacent column at the same row height), widening
            // pulls that unrelated column in too.
            let anchor_union = union_of(anchors.iter().copied()).unwrap();
            let widen_left = anchor_union.left().value - ROW_WIDEN_X_PT;
            let widen_right = anchor_union.right().value + ROW_WIDEN_X_PT;

            // The heading's own row gets a wider berth of its own — banner
            // text like a trailing "(AS OF AUG. 22)" can sit further from
            // the matched substring than other anchors reach — but bounded
            // to a margin around the heading's OWN box specifically, not
            // unlimited: unlimited width risks a single stray same-row
            // match anywhere on the page dragging the crop's bounding
            // rectangle (and everything inside it) across the whole page.
            let heading_widen_left = heading_rect.left().value - HEADING_ROW_WIDEN_LEFT_PT;
            let heading_widen_right = heading_rect.right().value + HEADING_ROW_WIDEN_RIGHT_PT;

            let mut widened = anchors.clone();
            for a in &anchors {
                let is_heading_row = rows_overlap(a, &heading_rect, ROW_TOLERANCE_PT);
                for seg in &all_segments {
                    if rows_overlap(a, seg, ROW_TOLERANCE_PT) {
                        let x_ok = if is_heading_row {
                            seg.left().value < heading_widen_right && seg.right().value > heading_widen_left
                        } else {
                            seg.left().value < widen_right && seg.right().value > widen_left
                        };
                        if x_ok {
                            widened.push(*seg);
                        }
                    }
                }
            }

            let mut union = widened[0];
            for r in &widened[1..] {
                union = merge(union, *r);
            }

            // Cap total height (anchored at the heading's own top) so a
            // stray match far below (e.g. an unrelated footer) can't pull
            // the crop down indefinitely.
            let min_bottom = heading_rect.top().value - MAX_HEIGHT_PT;
            let final_bottom = union.bottom().value.max(min_bottom);

            let png = render_crop(&page, union, final_bottom)?;
            if let Some(png) = png {
                return Ok(Some(png));
            }
        }
    }

    Ok(None)
}

fn render_crop(page: &PdfPage, union: PdfRect, final_bottom: f32) -> anyhow::Result<Option<Vec<u8>>> {
    let page_width = page.width().value;
    let page_height = page.height().value;
    let scale = RENDER_WIDTH_PX as f32 / page_width;

    let render_cfg = PdfRenderConfig::new()
        .set_target_width(RENDER_WIDTH_PX)
        .set_maximum_height(RENDER_MAX_HEIGHT_PX);
    let bitmap = page.render_with_config(&render_cfg)?;
    let img = bitmap.as_image();

    let crop_top_pdf = union.top().value + PAD_PT;
    let crop_bottom_pdf = final_bottom - PAD_PT;
    let crop_left_pdf = union.left().value - PAD_PT;
    let crop_right_pdf = union.right().value + PAD_PT;

    let px_top = ((page_height - crop_top_pdf) * scale).max(0.0) as u32;
    let px_bottom = ((page_height - crop_bottom_pdf) * scale).min(img.height() as f32) as u32;
    let px_left = (crop_left_pdf * scale).max(0.0) as u32;
    let px_right = (crop_right_pdf * scale).min(img.width() as f32) as u32;

    if px_bottom <= px_top || px_right <= px_left {
        return Ok(None);
    }

    let cropped = image::imageops::crop_imm(&img, px_left, px_top, px_right - px_left, px_bottom - px_top).to_image();

    let mut png_bytes = Vec::new();
    cropped.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)?;
    Ok(Some(png_bytes))
}

fn rows_overlap(a: &PdfRect, b: &PdfRect, tolerance: f32) -> bool {
    let a_top = a.top().value + tolerance;
    let a_bottom = a.bottom().value - tolerance;
    let b_top = b.top().value;
    let b_bottom = b.bottom().value;
    a_bottom <= b_top && b_bottom <= a_top
}

fn union_of(rects: impl Iterator<Item = PdfRect>) -> Option<PdfRect> {
    let mut acc: Option<PdfRect> = None;
    for r in rects {
        acc = Some(match acc {
            None => r,
            Some(a) => merge(a, r),
        });
    }
    acc
}

fn merge(a: PdfRect, b: PdfRect) -> PdfRect {
    PdfRect::new(
        PdfPoints::new(a.bottom().value.min(b.bottom().value)),
        PdfPoints::new(a.left().value.min(b.left().value)),
        PdfPoints::new(a.top().value.max(b.top().value)),
        PdfPoints::new(a.right().value.max(b.right().value)),
    )
}

#[cfg(test)]
mod real_pdf_tests {
    use super::*;

    const DOCS: &[(&str, &str)] = &[
        ("tampa1", "https://cdn1.sportngin.com/attachments/document/4dd4-3624695/Tampa_Bay_Sun_FC_Match_Notes.pdf"),
        ("tampa2", "https://cdn2.sportngin.com/attachments/document/20ad-3616759/Tampa_Bay_Sun_FC_Match_Notes.pdf"),
        ("tampa3", "https://cdn2.sportngin.com/attachments/document/be93-3626744/Tampa_Bay_Sun_FC_Match_Notes.pdf"),
        ("tampa4", "https://cdn3.sportngin.com/attachments/document/71d0-3613658/Tampa_Bay_Sun_FC_Match_Notes.pdf"),
        ("tampa5", "https://cdn3.sportngin.com/attachments/document/75ce-3619755/Tampa_Bay_Sun_FC_Match_Notes.pdf"),
        ("carolina", "https://cdn1.sportngin.com/attachments/document/6384-3619635/CARvTB_Game_Notes_8.29.pdf"),
        ("dallas", "https://cdn1.sportngin.com/attachments/document/5337-3616975/082226_MatchNotes_DTFCvsCarolinaAscent_FINAL.pdf"),
        ("brooklyn", "https://cdn3.sportngin.com/attachments/document/b2d6-3616239/BKFC_W_vs_DC_Game_Notes_82126.pdf"),
        ("sportingjax", "https://cdn2.sportngin.com/attachments/document/24da-3613507/Gainbridge_Super_League_Away_Match_Notes_8.15.26.pdf"),
        ("lexington", "https://cdn1.sportngin.com/attachments/document/f070-3619639/8.29.26_LEXvsDC_MatchNotes.pdf"),
        ("dcpower", "https://cdn3.sportngin.com/attachments/document/0821-3613980/DC_Power_Game_Notes_8.15.26.pdf"),
        ("ftlauderdale", "https://cdn3.sportngin.com/attachments/document/7371-3613469/1_-_FTLvsJAX_8.15.pdf"),
    ];

    #[tokio::test]
    #[ignore]
    async fn crop_all_real_docs() {
        let client = Client::new();
        let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("crop_check");
        std::fs::create_dir_all(&out_dir).unwrap();

        for (name, url) in DOCS {
            match availability_screenshot(&client, url).await {
                Ok(Some(png)) => {
                    let path = out_dir.join(format!("{name}.png"));
                    std::fs::write(&path, &png).unwrap();
                    println!("{name}: OK ({} bytes) -> {}", png.len(), path.display());
                }
                Ok(None) => println!("{name}: NO CROP FOUND"),
                Err(e) => println!("{name}: ERROR {e}"),
            }
        }
    }
}

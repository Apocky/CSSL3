//! TrueType font parser — minimal subset.
//!
//! § SCOPE (stage-0)
//!   - Parses the TTF / OpenType file header (offset table) + table
//!     directory.
//!   - Parses required tables : `head` (font header), `maxp` (max profile),
//!     `hhea` (horizontal header), `hmtx` (horizontal metrics), `name`
//!     (naming), `cmap` (character-to-glyph map ; format 4 only at
//!     stage-0), and `glyf` (glyph data ; simple-glyph outlines via
//!     `loca`).
//!
//! § DELIBERATELY DEFERRED (stage-0)
//!   - GPOS / GSUB layout tables (kerning, ligatures, complex scripts)
//!   - CFF / CFF2 (Compact Font Format ; OpenType-with-PostScript outlines)
//!   - Composite glyphs in `glyf` (only simple glyphs read at stage-0)
//!   - Variable fonts (`fvar` / `gvar`)
//!   - Bitmap glyphs (`EBLC` / `EBDT` / `sbix`)
//!   - Color fonts (`COLR` / `CPAL`)
//!
//! § SPEC
//!   ISO/IEC 14496-22 (OpenType) + Apple TrueType reference manual.
//!
//! § PRIME-DIRECTIVE
//!   The parser caps glyph index reads at the `maxp.numGlyphs` value
//!   reported by the font ; out-of-range indices yield `InvalidValue`.
//!   No system-font enumeration, no font-cache surveillance.

use crate::error::{AssetError, Result};

/// TrueType OFFSET-TABLE scaler-type for 1.0 fonts (TTF).
pub const SCALER_TTF: [u8; 4] = [0x00, 0x01, 0x00, 0x00];
/// OpenType OFFSET-TABLE scaler-type for `OTTO` (CFF outlines, deferred).
pub const SCALER_OTTO: [u8; 4] = *b"OTTO";
/// Apple TTF scaler-type `true`.
pub const SCALER_TRUE: [u8; 4] = *b"true";

/// Decoded font header subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontHeader {
    /// Font's design units per em-square (typical 1000 or 2048).
    pub units_per_em: u16,
    /// Stable identifier ; useful for cache-keying.
    pub font_revision: i32,
    /// Bounding box X-min in font units.
    pub x_min: i16,
    /// Bounding box Y-min in font units.
    pub y_min: i16,
    /// Bounding box X-max in font units.
    pub x_max: i16,
    /// Bounding box Y-max in font units.
    pub y_max: i16,
    /// Index-to-loc format : 0 = short (2-byte offsets), 1 = long (4-byte).
    pub index_to_loc_format: i16,
}

/// Decoded horizontal-header subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoriHeader {
    /// Distance from baseline to highest ascender (font units).
    pub ascender: i16,
    /// Distance from baseline to lowest descender (font units).
    pub descender: i16,
    /// Suggested line gap (font units).
    pub line_gap: i16,
    /// Number of `hmtx` longHorMetric records ; remaining glyphs share
    /// the last advance.
    pub number_of_h_metrics: u16,
}

/// Per-glyph horizontal advance + side-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphMetric {
    /// Advance width in font units.
    pub advance_width: u16,
    /// Left side bearing in font units.
    pub left_side_bearing: i16,
}

/// Simple-glyph outline (no composite) : a list of contours, each a list
/// of points (x, y, on_curve_flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphOutline {
    /// Per-contour end-point indices (inclusive).
    pub contour_end_points: Vec<u16>,
    /// Flat list of points across all contours.
    pub points: Vec<GlyphPoint>,
    /// Glyph bounding box X-min.
    pub x_min: i16,
    /// Glyph bounding box Y-min.
    pub y_min: i16,
    /// Glyph bounding box X-max.
    pub x_max: i16,
    /// Glyph bounding box Y-max.
    pub y_max: i16,
}

/// One point on a glyph outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphPoint {
    /// X coordinate (font units).
    pub x: i16,
    /// Y coordinate (font units).
    pub y: i16,
    /// True if the point lies on the curve ; false = control point.
    pub on_curve: bool,
}

/// Parsed TTF / OpenType font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtfFont {
    /// Font header.
    pub head: FontHeader,
    /// Horizontal header.
    pub hhea: HoriHeader,
    /// Total glyph count from `maxp`.
    pub num_glyphs: u16,
    /// Per-glyph horizontal metrics.
    pub h_metrics: Vec<GlyphMetric>,
    /// Per-glyph offsets into the `glyf` table (length = num_glyphs + 1).
    pub loca: Vec<u32>,
    /// Raw bytes of the `glyf` table (parsed lazily by `glyph_outline`).
    pub glyf: Vec<u8>,
    /// Stored copy of the cmap "format 4" mapping, if present.
    pub cmap_format4: Option<Cmap4>,
    /// Family-name string (stage-0 reads name-id 1, English).
    pub family_name: Option<String>,
}

/// `cmap` format-4 data subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmap4 {
    /// segCountX2 / 2 segments. Each segment maps `[start_code,
    /// end_code]` to glyph IDs.
    pub end_codes: Vec<u16>,
    /// Per-segment start codes.
    pub start_codes: Vec<u16>,
    /// Per-segment id-deltas.
    pub id_deltas: Vec<i16>,
    /// Per-segment id-range-offsets (raw ; resolution at lookup time).
    pub id_range_offsets: Vec<u16>,
    /// Glyph-id array (resolved per id-range-offsets).
    pub glyph_id_array: Vec<u16>,
}

impl Cmap4 {
    /// Map a Unicode code point (`u32`) to a glyph index. Returns 0
    /// (the .notdef glyph) for unmapped code points.
    #[must_use]
    pub fn lookup(&self, code: u32) -> u16 {
        if code > u32::from(u16::MAX) {
            return 0;
        }
        let cp = code as u16;
        for (i, &end) in self.end_codes.iter().enumerate() {
            if cp > end {
                continue;
            }
            let start = self.start_codes[i];
            if cp < start {
                return 0;
            }
            let offset = self.id_range_offsets[i];
            if offset == 0 {
                return cp.wrapping_add(self.id_deltas[i] as u16);
            }
            // The format-4 idRangeOffset trick : the glyphIdArray
            // index = idRangeOffset[i] / 2 + (cp - start) -
            // (segCount - i).  In our flat representation we
            // pre-compute this offset against `glyph_id_array`.
            let seg_count = self.end_codes.len() as u16;
            let half_offset = offset / 2;
            let idx = (cp.wrapping_sub(start) as i32) + (half_offset as i32)
                - ((seg_count as i32) - (i as i32));
            if idx < 0 || (idx as usize) >= self.glyph_id_array.len() {
                return 0;
            }
            let gid = self.glyph_id_array[idx as usize];
            return if gid == 0 {
                0
            } else {
                gid.wrapping_add(self.id_deltas[i] as u16)
            };
        }
        0
    }
}

impl TtfFont {
    /// Look up the glyph index for a Unicode code point. Returns 0
    /// (.notdef) when no cmap or no mapping.
    #[must_use]
    pub fn glyph_index(&self, code: u32) -> u16 {
        match &self.cmap_format4 {
            Some(c) => c.lookup(code),
            None => 0,
        }
    }

    /// Horizontal metric for a glyph index. Glyphs beyond
    /// `hhea.number_of_h_metrics` share the last advance with their
    /// own side bearing (stage-0 simplification : we just clamp to the
    /// last metric and report 0 left-side-bearing for non-recorded
    /// glyphs).
    #[must_use]
    pub fn glyph_metric(&self, glyph: u16) -> GlyphMetric {
        if self.h_metrics.is_empty() {
            return GlyphMetric {
                advance_width: 0,
                left_side_bearing: 0,
            };
        }
        let idx = (glyph as usize).min(self.h_metrics.len() - 1);
        self.h_metrics[idx]
    }

    /// Parse the outline of glyph `glyph_id`. Returns an empty outline
    /// for glyphs whose `loca` entry is zero-length (PRIME `notdef` etc.).
    pub fn glyph_outline(&self, glyph_id: u16) -> Result<GlyphOutline> {
        if u32::from(glyph_id) >= u32::from(self.num_glyphs) {
            return Err(AssetError::invalid(
                "TTF",
                "glyph",
                format!("glyph_id {glyph_id} >= num_glyphs {}", self.num_glyphs),
            ));
        }
        let g = glyph_id as usize;
        if g + 1 >= self.loca.len() {
            return Err(AssetError::invalid(
                "TTF",
                "loca",
                "out of range glyph index for loca",
            ));
        }
        let start = self.loca[g] as usize;
        let end = self.loca[g + 1] as usize;
        if start == end {
            // Empty glyph (typical for space).
            return Ok(GlyphOutline {
                contour_end_points: Vec::new(),
                points: Vec::new(),
                x_min: 0,
                y_min: 0,
                x_max: 0,
                y_max: 0,
            });
        }
        if end > self.glyf.len() {
            return Err(AssetError::truncated("TTF/glyf", end, self.glyf.len()));
        }
        parse_simple_glyph(&self.glyf[start..end])
    }
}

// ─────────────────────────────────────────────────────────────────────────
// § PARSER
// ─────────────────────────────────────────────────────────────────────────

/// Parse a TTF / OpenType byte stream.
pub fn parse(bytes: &[u8]) -> Result<TtfFont> {
    if bytes.len() < 12 {
        return Err(AssetError::truncated("TTF/header", 12, bytes.len()));
    }
    let scaler = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if scaler != SCALER_TTF && scaler != SCALER_TRUE {
        if scaler == SCALER_OTTO {
            return Err(AssetError::unsupported(
                "TTF",
                "CFF outlines (OTTO scaler) deferred at stage-0",
            ));
        }
        return Err(AssetError::bad_magic("TTF", &bytes[..4]));
    }
    let num_tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
    if num_tables == 0 {
        return Err(AssetError::invalid("TTF", "header", "numTables=0"));
    }
    // Table directory : 16 bytes per table.
    let dir_end = 12 + num_tables * 16;
    if bytes.len() < dir_end {
        return Err(AssetError::truncated(
            "TTF/table-directory",
            dir_end,
            bytes.len(),
        ));
    }
    let mut head_range: Option<(usize, usize)> = None;
    let mut maxp_range: Option<(usize, usize)> = None;
    let mut hhea_range: Option<(usize, usize)> = None;
    let mut hmtx_range: Option<(usize, usize)> = None;
    let mut loca_range: Option<(usize, usize)> = None;
    let mut glyf_range: Option<(usize, usize)> = None;
    let mut cmap_range: Option<(usize, usize)> = None;
    let mut name_range: Option<(usize, usize)> = None;

    for i in 0..num_tables {
        let off = 12 + i * 16;
        let tag = [bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]];
        let table_off = u32::from_be_bytes([
            bytes[off + 8],
            bytes[off + 9],
            bytes[off + 10],
            bytes[off + 11],
        ]) as usize;
        let table_len = u32::from_be_bytes([
            bytes[off + 12],
            bytes[off + 13],
            bytes[off + 14],
            bytes[off + 15],
        ]) as usize;
        let table_end = table_off
            .checked_add(table_len)
            .ok_or_else(|| AssetError::invalid("TTF", "table-dir", "offset+len overflow"))?;
        if table_end > bytes.len() {
            return Err(AssetError::truncated(
                "TTF/table-body",
                table_end,
                bytes.len(),
            ));
        }
        let range = (table_off, table_end);
        match &tag {
            b"head" => head_range = Some(range),
            b"maxp" => maxp_range = Some(range),
            b"hhea" => hhea_range = Some(range),
            b"hmtx" => hmtx_range = Some(range),
            b"loca" => loca_range = Some(range),
            b"glyf" => glyf_range = Some(range),
            b"cmap" => cmap_range = Some(range),
            b"name" => name_range = Some(range),
            _ => {}
        }
    }

    let head_data = take_required(bytes, head_range, "head")?;
    let maxp_data = take_required(bytes, maxp_range, "maxp")?;
    let hhea_data = take_required(bytes, hhea_range, "hhea")?;
    let hmtx_data = take_required(bytes, hmtx_range, "hmtx")?;
    let loca_data = take_required(bytes, loca_range, "loca")?;
    let glyf_data = take_required(bytes, glyf_range, "glyf")?;
    let cmap_data = cmap_range.map(|(s, e)| &bytes[s..e]);
    let name_data = name_range.map(|(s, e)| &bytes[s..e]);

    let head = parse_head(head_data)?;
    let num_glyphs = parse_maxp_num_glyphs(maxp_data)?;
    let hhea = parse_hhea(hhea_data)?;
    let h_metrics = parse_hmtx(hmtx_data, hhea.number_of_h_metrics, num_glyphs)?;
    let loca = parse_loca(loca_data, num_glyphs, head.index_to_loc_format)?;
    let glyf = glyf_data.to_vec();
    let cmap_format4 = match cmap_data {
        Some(d) => parse_cmap_format4(d)?,
        None => None,
    };
    let family_name = match name_data {
        Some(d) => parse_name_family(d).ok(),
        None => None,
    };

    Ok(TtfFont {
        head,
        hhea,
        num_glyphs,
        h_metrics,
        loca,
        glyf,
        cmap_format4,
        family_name,
    })
}

fn take_required<'a>(
    bytes: &'a [u8],
    range: Option<(usize, usize)>,
    name: &'static str,
) -> Result<&'a [u8]> {
    match range {
        Some((s, e)) => Ok(&bytes[s..e]),
        None => Err(AssetError::invalid(
            "TTF",
            "table-directory",
            format!("missing required table `{name}`"),
        )),
    }
}

fn parse_head(data: &[u8]) -> Result<FontHeader> {
    if data.len() < 54 {
        return Err(AssetError::truncated("TTF/head", 54, data.len()));
    }
    let font_revision = i32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let units_per_em = u16::from_be_bytes([data[18], data[19]]);
    let x_min = i16::from_be_bytes([data[36], data[37]]);
    let y_min = i16::from_be_bytes([data[38], data[39]]);
    let x_max = i16::from_be_bytes([data[40], data[41]]);
    let y_max = i16::from_be_bytes([data[42], data[43]]);
    let index_to_loc_format = i16::from_be_bytes([data[50], data[51]]);
    if !(16..=16384).contains(&units_per_em) {
        return Err(AssetError::invalid(
            "TTF",
            "head/unitsPerEm",
            format!("out of range : {units_per_em}"),
        ));
    }
    if !(0..=1).contains(&index_to_loc_format) {
        return Err(AssetError::invalid(
            "TTF",
            "head/indexToLocFormat",
            format!("out of range : {index_to_loc_format}"),
        ));
    }
    Ok(FontHeader {
        units_per_em,
        font_revision,
        x_min,
        y_min,
        x_max,
        y_max,
        index_to_loc_format,
    })
}

fn parse_maxp_num_glyphs(data: &[u8]) -> Result<u16> {
    if data.len() < 6 {
        return Err(AssetError::truncated("TTF/maxp", 6, data.len()));
    }
    let n = u16::from_be_bytes([data[4], data[5]]);
    if n == 0 {
        return Err(AssetError::invalid("TTF", "maxp", "numGlyphs=0"));
    }
    Ok(n)
}

fn parse_hhea(data: &[u8]) -> Result<HoriHeader> {
    if data.len() < 36 {
        return Err(AssetError::truncated("TTF/hhea", 36, data.len()));
    }
    let ascender = i16::from_be_bytes([data[4], data[5]]);
    let descender = i16::from_be_bytes([data[6], data[7]]);
    let line_gap = i16::from_be_bytes([data[8], data[9]]);
    let number_of_h_metrics = u16::from_be_bytes([data[34], data[35]]);
    if number_of_h_metrics == 0 {
        return Err(AssetError::invalid("TTF", "hhea", "numberOfHMetrics=0"));
    }
    Ok(HoriHeader {
        ascender,
        descender,
        line_gap,
        number_of_h_metrics,
    })
}

fn parse_hmtx(data: &[u8], num_metrics: u16, num_glyphs: u16) -> Result<Vec<GlyphMetric>> {
    let nm = num_metrics as usize;
    let ng = num_glyphs as usize;
    let needed = nm * 4 + (ng.saturating_sub(nm)) * 2;
    if data.len() < needed {
        return Err(AssetError::truncated("TTF/hmtx", needed, data.len()));
    }
    let mut out = Vec::with_capacity(ng);
    for i in 0..nm {
        let off = i * 4;
        let aw = u16::from_be_bytes([data[off], data[off + 1]]);
        let lsb = i16::from_be_bytes([data[off + 2], data[off + 3]]);
        out.push(GlyphMetric {
            advance_width: aw,
            left_side_bearing: lsb,
        });
    }
    // Trailing glyphs share the last advance ; their LSB sits in a
    // packed array right after the metrics.
    let last_aw = out.last().map(|m| m.advance_width).unwrap_or(0);
    for j in nm..ng {
        let off = nm * 4 + (j - nm) * 2;
        let lsb = i16::from_be_bytes([data[off], data[off + 1]]);
        out.push(GlyphMetric {
            advance_width: last_aw,
            left_side_bearing: lsb,
        });
    }
    Ok(out)
}

fn parse_loca(data: &[u8], num_glyphs: u16, idx_to_loc: i16) -> Result<Vec<u32>> {
    let n = (num_glyphs as usize) + 1;
    if idx_to_loc == 0 {
        // Short : 2-byte offsets, divided by 2.
        let needed = n * 2;
        if data.len() < needed {
            return Err(AssetError::truncated("TTF/loca-short", needed, data.len()));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let v = u16::from_be_bytes([data[i * 2], data[i * 2 + 1]]) as u32;
            out.push(v * 2);
        }
        Ok(out)
    } else {
        // Long : 4-byte offsets.
        let needed = n * 4;
        if data.len() < needed {
            return Err(AssetError::truncated("TTF/loca-long", needed, data.len()));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let off = i * 4;
            out.push(u32::from_be_bytes([
                data[off],
                data[off + 1],
                data[off + 2],
                data[off + 3],
            ]));
        }
        Ok(out)
    }
}

fn parse_cmap_format4(data: &[u8]) -> Result<Option<Cmap4>> {
    if data.len() < 4 {
        return Err(AssetError::truncated("TTF/cmap", 4, data.len()));
    }
    let num_subtables = u16::from_be_bytes([data[2], data[3]]) as usize;
    let mut chosen: Option<&[u8]> = None;
    for i in 0..num_subtables {
        let off = 4 + i * 8;
        if off + 8 > data.len() {
            return Err(AssetError::truncated(
                "TTF/cmap-subtable",
                off + 8,
                data.len(),
            ));
        }
        let _platform_id = u16::from_be_bytes([data[off], data[off + 1]]);
        let _encoding_id = u16::from_be_bytes([data[off + 2], data[off + 3]]);
        let table_off =
            u32::from_be_bytes([data[off + 4], data[off + 5], data[off + 6], data[off + 7]])
                as usize;
        if table_off + 4 > data.len() {
            continue;
        }
        let format = u16::from_be_bytes([data[table_off], data[table_off + 1]]);
        if format == 4 {
            chosen = Some(&data[table_off..]);
            break;
        }
    }
    let sub = match chosen {
        Some(s) => s,
        None => return Ok(None),
    };
    if sub.len() < 14 {
        return Err(AssetError::truncated("TTF/cmap-format4", 14, sub.len()));
    }
    let length = u16::from_be_bytes([sub[2], sub[3]]) as usize;
    if sub.len() < length {
        return Err(AssetError::truncated(
            "TTF/cmap-format4-len",
            length,
            sub.len(),
        ));
    }
    let seg_count_x2 = u16::from_be_bytes([sub[6], sub[7]]) as usize;
    if seg_count_x2 % 2 != 0 {
        return Err(AssetError::invalid(
            "TTF",
            "cmap-format4",
            format!("segCountX2 not even : {seg_count_x2}"),
        ));
    }
    let seg_count = seg_count_x2 / 2;
    if seg_count == 0 {
        return Err(AssetError::invalid("TTF", "cmap-format4", "segCount=0"));
    }
    // Layout : end_codes[seg_count] u16, reserved-pad u16,
    //          start_codes[seg_count] u16,
    //          id_deltas[seg_count] i16,
    //          id_range_offsets[seg_count] u16,
    //          glyph_id_array[N] u16
    let mut p = 14;
    let mut end_codes = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        if p + 2 > length {
            return Err(AssetError::truncated(
                "TTF/cmap-format4-endCodes",
                p + 2,
                length,
            ));
        }
        end_codes.push(u16::from_be_bytes([sub[p], sub[p + 1]]));
        p += 2;
    }
    p += 2; // reserved pad
    let mut start_codes = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        if p + 2 > length {
            return Err(AssetError::truncated(
                "TTF/cmap-format4-startCodes",
                p + 2,
                length,
            ));
        }
        start_codes.push(u16::from_be_bytes([sub[p], sub[p + 1]]));
        p += 2;
    }
    let mut id_deltas = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        if p + 2 > length {
            return Err(AssetError::truncated(
                "TTF/cmap-format4-idDeltas",
                p + 2,
                length,
            ));
        }
        id_deltas.push(i16::from_be_bytes([sub[p], sub[p + 1]]));
        p += 2;
    }
    let mut id_range_offsets = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        if p + 2 > length {
            return Err(AssetError::truncated(
                "TTF/cmap-format4-idRangeOffsets",
                p + 2,
                length,
            ));
        }
        id_range_offsets.push(u16::from_be_bytes([sub[p], sub[p + 1]]));
        p += 2;
    }
    let mut glyph_id_array = Vec::new();
    while p + 2 <= length {
        glyph_id_array.push(u16::from_be_bytes([sub[p], sub[p + 1]]));
        p += 2;
    }
    Ok(Some(Cmap4 {
        end_codes,
        start_codes,
        id_deltas,
        id_range_offsets,
        glyph_id_array,
    }))
}

fn parse_name_family(data: &[u8]) -> Result<String> {
    if data.len() < 6 {
        return Err(AssetError::truncated("TTF/name", 6, data.len()));
    }
    let count = u16::from_be_bytes([data[2], data[3]]) as usize;
    let storage_off = u16::from_be_bytes([data[4], data[5]]) as usize;
    let records_start = 6;
    let records_end = records_start + count * 12;
    if records_end > data.len() {
        return Err(AssetError::truncated(
            "TTF/name-records",
            records_end,
            data.len(),
        ));
    }
    // Look for name-id 1 (font family) ; prefer Mac-Roman (platform 1)
    // ASCII or Microsoft Unicode (platform 3, encoding 1).
    for i in 0..count {
        let off = records_start + i * 12;
        let platform_id = u16::from_be_bytes([data[off], data[off + 1]]);
        let encoding_id = u16::from_be_bytes([data[off + 2], data[off + 3]]);
        let _language_id = u16::from_be_bytes([data[off + 4], data[off + 5]]);
        let name_id = u16::from_be_bytes([data[off + 6], data[off + 7]]);
        let str_len = u16::from_be_bytes([data[off + 8], data[off + 9]]) as usize;
        let str_off = u16::from_be_bytes([data[off + 10], data[off + 11]]) as usize;
        if name_id != 1 {
            continue;
        }
        let abs_off = storage_off + str_off;
        if abs_off + str_len > data.len() {
            continue;
        }
        let raw = &data[abs_off..abs_off + str_len];
        if platform_id == 3 && encoding_id == 1 {
            // UTF-16BE.
            let mut chars = Vec::with_capacity(str_len / 2);
            let mut k = 0;
            while k + 1 < raw.len() {
                let c = u16::from_be_bytes([raw[k], raw[k + 1]]);
                chars.push(c);
                k += 2;
            }
            if let Ok(s) = String::from_utf16(&chars) {
                return Ok(s);
            }
        } else if platform_id == 1 && encoding_id == 0 {
            // Mac-Roman ; ASCII subset is fine for stage-0.
            if let Ok(s) = std::str::from_utf8(raw) {
                return Ok(s.to_string());
            }
        }
    }
    Err(AssetError::invalid(
        "TTF",
        "name",
        "no readable family-name record found",
    ))
}

fn parse_simple_glyph(data: &[u8]) -> Result<GlyphOutline> {
    if data.len() < 10 {
        return Err(AssetError::truncated("TTF/glyph-header", 10, data.len()));
    }
    let n_contours = i16::from_be_bytes([data[0], data[1]]);
    let x_min = i16::from_be_bytes([data[2], data[3]]);
    let y_min = i16::from_be_bytes([data[4], data[5]]);
    let x_max = i16::from_be_bytes([data[6], data[7]]);
    let y_max = i16::from_be_bytes([data[8], data[9]]);
    if n_contours < 0 {
        return Err(AssetError::unsupported(
            "TTF",
            "composite glyph (deferred at stage-0)",
        ));
    }
    let n_contours = n_contours as usize;
    let mut p = 10;
    let mut end_pts = Vec::with_capacity(n_contours);
    for _ in 0..n_contours {
        if p + 2 > data.len() {
            return Err(AssetError::truncated("TTF/glyph-endPts", p + 2, data.len()));
        }
        end_pts.push(u16::from_be_bytes([data[p], data[p + 1]]));
        p += 2;
    }
    if p + 2 > data.len() {
        return Err(AssetError::truncated(
            "TTF/glyph-instr-len",
            p + 2,
            data.len(),
        ));
    }
    let instr_len = u16::from_be_bytes([data[p], data[p + 1]]) as usize;
    p += 2;
    p = p
        .checked_add(instr_len)
        .ok_or_else(|| AssetError::invalid("TTF", "glyph", "instr-len overflow"))?;
    if p > data.len() {
        return Err(AssetError::truncated(
            "TTF/glyph-instructions",
            p,
            data.len(),
        ));
    }
    let n_points = end_pts.last().map(|&e| e as usize + 1).unwrap_or(0);
    if n_contours == 0 {
        return Ok(GlyphOutline {
            contour_end_points: Vec::new(),
            points: Vec::new(),
            x_min,
            y_min,
            x_max,
            y_max,
        });
    }
    // Read flags array (variable length).
    let mut flags = Vec::with_capacity(n_points);
    while flags.len() < n_points {
        if p >= data.len() {
            return Err(AssetError::truncated("TTF/glyph-flags", p, data.len()));
        }
        let f = data[p];
        p += 1;
        flags.push(f);
        if f & 0x08 != 0 {
            // REPEAT_FLAG
            if p >= data.len() {
                return Err(AssetError::truncated(
                    "TTF/glyph-flags-repeat",
                    p,
                    data.len(),
                ));
            }
            let repeat = data[p] as usize;
            p += 1;
            for _ in 0..repeat {
                if flags.len() >= n_points {
                    break;
                }
                flags.push(f);
            }
        }
    }
    if flags.len() != n_points {
        return Err(AssetError::invalid(
            "TTF",
            "glyph-flags",
            format!("expected {n_points} flags, got {}", flags.len()),
        ));
    }
    // Read X coordinates (delta-encoded).
    let mut xs = Vec::with_capacity(n_points);
    let mut x: i32 = 0;
    for &f in &flags {
        let dx = if f & 0x02 != 0 {
            // 1-byte X
            if p >= data.len() {
                return Err(AssetError::truncated("TTF/glyph-x-1byte", p, data.len()));
            }
            let v = data[p] as i32;
            p += 1;
            if f & 0x10 != 0 {
                v
            } else {
                -v
            }
        } else if f & 0x10 != 0 {
            // X = previous (delta=0)
            0
        } else {
            // 2-byte signed X.
            if p + 2 > data.len() {
                return Err(AssetError::truncated(
                    "TTF/glyph-x-2byte",
                    p + 2,
                    data.len(),
                ));
            }
            let v = i16::from_be_bytes([data[p], data[p + 1]]) as i32;
            p += 2;
            v
        };
        x += dx;
        xs.push(x);
    }
    // Read Y coordinates (delta-encoded).
    let mut ys = Vec::with_capacity(n_points);
    let mut y: i32 = 0;
    for &f in &flags {
        let dy = if f & 0x04 != 0 {
            // 1-byte Y
            if p >= data.len() {
                return Err(AssetError::truncated("TTF/glyph-y-1byte", p, data.len()));
            }
            let v = data[p] as i32;
            p += 1;
            if f & 0x20 != 0 {
                v
            } else {
                -v
            }
        } else if f & 0x20 != 0 {
            // Y = previous (delta=0)
            0
        } else {
            if p + 2 > data.len() {
                return Err(AssetError::truncated(
                    "TTF/glyph-y-2byte",
                    p + 2,
                    data.len(),
                ));
            }
            let v = i16::from_be_bytes([data[p], data[p + 1]]) as i32;
            p += 2;
            v
        };
        y += dy;
        ys.push(y);
    }
    // Build points.
    let mut points = Vec::with_capacity(n_points);
    for i in 0..n_points {
        let on_curve = flags[i] & 0x01 != 0;
        points.push(GlyphPoint {
            x: xs[i].clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            y: ys[i].clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            on_curve,
        });
    }
    Ok(GlyphOutline {
        contour_end_points: end_pts,
        points,
        x_min,
        y_min,
        x_max,
        y_max,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal in-memory TTF for testing. The font has a single
    /// triangular glyph (3 on-curve points) at glyph-id 1, with .notdef
    /// at glyph-id 0. cmap maps U+0041 ('A') to glyph 1.
    fn synthesize_minimal_ttf() -> Vec<u8> {
        // Helpers.
        let pad4 = |v: &mut Vec<u8>| {
            while v.len() % 4 != 0 {
                v.push(0);
            }
        };

        // ─── glyf table ──────────────────────────────────────────────
        // Glyph 0 : empty (.notdef ; zero-length).
        // Glyph 1 : triangle. n_contours=1, bbox=(0,0,1000,1000).
        //           One contour ending at point 2 (3 points).
        //           Instruction length = 0.
        //           Flags : 0x01 ON_CURVE ; for simplicity X_SHORT=0,
        //           Y_SHORT=0 → 2-byte signed deltas.
        //           Points : (0,0), (1000,0), (500,1000).
        let mut glyf = Vec::new();
        // Glyph 1 starts at offset 0.
        // n_contours = 1
        glyf.extend_from_slice(&1i16.to_be_bytes());
        // bbox
        glyf.extend_from_slice(&0i16.to_be_bytes());
        glyf.extend_from_slice(&0i16.to_be_bytes());
        glyf.extend_from_slice(&1000i16.to_be_bytes());
        glyf.extend_from_slice(&1000i16.to_be_bytes());
        // endPtsOfContours[0] = 2 (last point is index 2)
        glyf.extend_from_slice(&2u16.to_be_bytes());
        // instructionLength = 0
        glyf.extend_from_slice(&0u16.to_be_bytes());
        // flags : 3 × 0x01 (on-curve).
        glyf.extend_from_slice(&[0x01, 0x01, 0x01]);
        // X deltas (2-byte signed, since X_SHORT=0 + SAME_OR_POS=0 means
        // 2-byte signed delta) : 0, 1000, -500.
        glyf.extend_from_slice(&0i16.to_be_bytes());
        glyf.extend_from_slice(&1000i16.to_be_bytes());
        glyf.extend_from_slice(&(-500i16).to_be_bytes());
        // Y deltas : 0, 0, 1000.
        glyf.extend_from_slice(&0i16.to_be_bytes());
        glyf.extend_from_slice(&0i16.to_be_bytes());
        glyf.extend_from_slice(&1000i16.to_be_bytes());
        let glyph1_len = glyf.len() as u32;
        pad4(&mut glyf);
        // Glyph 0 (empty) : there is no body to write — `loca[0]==loca[1]`
        // signals empty. But we want glyph 0 to be empty AND glyph 1
        // populated, so we'll make `loca = [0, 0, glyph1_len_padded]`
        // with the assumption glyph 1 is at offset 0. That conflicts —
        // we instead place glyph 0 empty AT offset 0 (zero length), then
        // glyph 1 starting at 0 too, then final entry at glyph1_len.
        // The glyf table only contains glyph 1's bytes ; loca says
        // [0, 0, glyph1_len].

        // ─── loca table (long format) ───────────────────────────────
        let mut loca = Vec::new();
        loca.extend_from_slice(&0u32.to_be_bytes()); // glyph 0 start
        loca.extend_from_slice(&0u32.to_be_bytes()); // glyph 1 start (== glyph 0 end → glyph 0 empty)
        loca.extend_from_slice(&glyph1_len.to_be_bytes()); // glyph 2 = end of glyph 1

        // ─── head table ─────────────────────────────────────────────
        let mut head = vec![0u8; 54];
        // version 1.0 : major=1, minor=0
        head[0] = 0;
        head[1] = 1;
        head[2] = 0;
        head[3] = 0;
        // fontRevision (1.0).
        head[4] = 0;
        head[5] = 1;
        head[6] = 0;
        head[7] = 0;
        // checksumAdjustment, magicNumber - leave zeroed for stage-0
        // (parser doesn't validate them).
        // unitsPerEm = 1000 at offset 18.
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        // bbox.
        head[36..38].copy_from_slice(&0i16.to_be_bytes());
        head[38..40].copy_from_slice(&0i16.to_be_bytes());
        head[40..42].copy_from_slice(&1000i16.to_be_bytes());
        head[42..44].copy_from_slice(&1000i16.to_be_bytes());
        // indexToLocFormat = 1 (long) at offset 50.
        head[50..52].copy_from_slice(&1i16.to_be_bytes());
        // glyphDataFormat at offset 52 = 0.

        // ─── maxp table ─────────────────────────────────────────────
        let mut maxp = vec![0u8; 32];
        // version = 1.0.
        maxp[0] = 0;
        maxp[1] = 1;
        maxp[2] = 0;
        maxp[3] = 0;
        // numGlyphs = 2 at offset 4.
        maxp[4..6].copy_from_slice(&2u16.to_be_bytes());

        // ─── hhea table ─────────────────────────────────────────────
        let mut hhea = vec![0u8; 36];
        // version = 1.0.
        hhea[0] = 0;
        hhea[1] = 1;
        // ascender / descender / lineGap.
        hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        hhea[8..10].copy_from_slice(&100i16.to_be_bytes());
        // numberOfHMetrics = 2 at offset 34.
        hhea[34..36].copy_from_slice(&2u16.to_be_bytes());

        // ─── hmtx table ─────────────────────────────────────────────
        // 2 longHorMetric entries (4 bytes each).
        let mut hmtx = Vec::new();
        // glyph 0 : aw=500, lsb=0
        hmtx.extend_from_slice(&500u16.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
        // glyph 1 : aw=1000, lsb=0
        hmtx.extend_from_slice(&1000u16.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());

        // ─── cmap table ─────────────────────────────────────────────
        // version (2) + numSubtables (2) + 1 record (8) + format4 body.
        // We map U+0041 → glyph 1. Format-4 with 2 segments :
        //   seg 0 : start=0x41, end=0x41, idDelta=0, idRangeOffset=4,
        //           glyph_id_array[0]=1
        //   seg 1 : start=0xFFFF, end=0xFFFF, idDelta=1, idRangeOffset=0
        //           (sentinel mandatory per OpenType spec)
        let mut cmap = Vec::new();
        cmap.extend_from_slice(&0u16.to_be_bytes()); // version
        cmap.extend_from_slice(&1u16.to_be_bytes()); // numSubtables
                                                     // record : platform=3, encoding=1, offset=12 (header is 12 bytes).
        cmap.extend_from_slice(&3u16.to_be_bytes());
        cmap.extend_from_slice(&1u16.to_be_bytes());
        cmap.extend_from_slice(&12u32.to_be_bytes());
        // format-4 body (compute length later).
        let body_start = cmap.len();
        let seg_count: u16 = 2;
        let seg_count_x2 = seg_count * 2;
        // Search range / entry selector / range shift : standard.
        // searchRange = 2 * 2^floor(log2(segCount))
        // For seg_count=2, floor(log2(2))=1, searchRange=4
        let search_range = 4u16;
        let entry_selector = 1u16;
        let range_shift = seg_count_x2 - search_range;
        // Reserve length placeholder.
        cmap.extend_from_slice(&0u16.to_be_bytes()); // format 4
        cmap.extend_from_slice(&0u16.to_be_bytes()); // length placeholder
        cmap.extend_from_slice(&0u16.to_be_bytes()); // language
        cmap.extend_from_slice(&seg_count_x2.to_be_bytes());
        cmap.extend_from_slice(&search_range.to_be_bytes());
        cmap.extend_from_slice(&entry_selector.to_be_bytes());
        cmap.extend_from_slice(&range_shift.to_be_bytes());
        // endCodes
        cmap.extend_from_slice(&0x41u16.to_be_bytes());
        cmap.extend_from_slice(&0xffffu16.to_be_bytes());
        // reservedPad
        cmap.extend_from_slice(&0u16.to_be_bytes());
        // startCodes
        cmap.extend_from_slice(&0x41u16.to_be_bytes());
        cmap.extend_from_slice(&0xffffu16.to_be_bytes());
        // idDeltas
        cmap.extend_from_slice(&0i16.to_be_bytes());
        cmap.extend_from_slice(&1i16.to_be_bytes());
        // idRangeOffsets
        cmap.extend_from_slice(&4u16.to_be_bytes());
        cmap.extend_from_slice(&0u16.to_be_bytes());
        // glyph_id_array : single entry → glyph 1.
        cmap.extend_from_slice(&1u16.to_be_bytes());
        let body_end = cmap.len();
        let body_len = (body_end - body_start) as u16;
        // Patch length at body_start + 2.
        cmap[body_start + 2..body_start + 4].copy_from_slice(&body_len.to_be_bytes());
        // Format header (first 2 bytes of body) was zero ; set to 4.
        cmap[body_start..body_start + 2].copy_from_slice(&4u16.to_be_bytes());

        // ─── name table ─────────────────────────────────────────────
        // count = 1, storageOffset = 6 + 12 = 18, then one record + one
        // string ("CSSL").
        let family = "CSSL";
        let mut name = Vec::new();
        name.extend_from_slice(&0u16.to_be_bytes()); // version
        name.extend_from_slice(&1u16.to_be_bytes()); // count
        name.extend_from_slice(&(6 + 12u16).to_be_bytes()); // storage offset
                                                            // record : platform=3, encoding=1, language=0, name=1, length=8
                                                            // (UTF-16BE bytes), offset=0.
        name.extend_from_slice(&3u16.to_be_bytes());
        name.extend_from_slice(&1u16.to_be_bytes());
        name.extend_from_slice(&0u16.to_be_bytes());
        name.extend_from_slice(&1u16.to_be_bytes());
        let utf16: Vec<u8> = family
            .encode_utf16()
            .flat_map(|c| c.to_be_bytes().to_vec())
            .collect();
        name.extend_from_slice(&(utf16.len() as u16).to_be_bytes());
        name.extend_from_slice(&0u16.to_be_bytes());
        name.extend_from_slice(&utf16);

        // ─── Build offset table ────────────────────────────────────
        // Compute offsets : sfnt offset table is 12 bytes ; table dir
        // is 16 bytes per record.
        let mut tables = Vec::<(&[u8; 4], Vec<u8>)>::new();
        // Tables in tag-sort order : cmap, glyf, head, hhea, hmtx, loca, maxp, name
        // but stage-0 parser doesn't care about order.
        tables.push((b"cmap", cmap));
        tables.push((b"glyf", glyf));
        tables.push((b"head", head));
        tables.push((b"hhea", hhea));
        tables.push((b"hmtx", hmtx));
        tables.push((b"loca", loca));
        tables.push((b"maxp", maxp));
        tables.push((b"name", name));
        // Remove name from count since `tables` length differs from
        // header table_count : update count to actual length.
        let table_count = tables.len() as u16;
        let header_len = 12 + (table_count as usize) * 16;

        // Compute table-body offsets, padded to 4 bytes between bodies.
        let mut bodies = Vec::new();
        let mut offsets = Vec::with_capacity(tables.len());
        let mut cursor = header_len;
        for (_tag, body) in &tables {
            offsets.push(cursor as u32);
            let body_len = body.len();
            cursor += body_len;
            // Pad to 4-byte alignment.
            while cursor % 4 != 0 {
                cursor += 1;
            }
        }

        // Emit header.
        let mut out = Vec::new();
        out.extend_from_slice(&SCALER_TTF);
        out.extend_from_slice(&table_count.to_be_bytes());
        // searchRange / entrySelector / rangeShift (parser doesn't read
        // these but spec-conformant).
        let mut searchable = 1u16;
        let mut sel = 0u16;
        while searchable * 2 <= table_count {
            searchable *= 2;
            sel += 1;
        }
        let search_range_h = searchable * 16;
        let range_shift_h = table_count * 16 - search_range_h;
        out.extend_from_slice(&search_range_h.to_be_bytes());
        out.extend_from_slice(&sel.to_be_bytes());
        out.extend_from_slice(&range_shift_h.to_be_bytes());
        // Emit table directory.
        for (i, (tag, body)) in tables.iter().enumerate() {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&0u32.to_be_bytes()); // checkSum (parser ignores)
            out.extend_from_slice(&offsets[i].to_be_bytes());
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        }
        // Emit padded bodies.
        for body in tables.iter().map(|(_, b)| b) {
            bodies.push(body);
        }
        let mut cursor = out.len();
        for body in bodies {
            out.extend_from_slice(body);
            cursor += body.len();
            while cursor % 4 != 0 {
                out.push(0);
                cursor += 1;
            }
        }
        out
    }

    #[test]
    fn parse_synthesized_minimal_font_succeeds() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).expect("synthesized TTF parses");
        assert_eq!(font.head.units_per_em, 1000);
        assert_eq!(font.num_glyphs, 2);
        assert_eq!(font.h_metrics.len(), 2);
        assert_eq!(font.h_metrics[1].advance_width, 1000);
        assert_eq!(font.hhea.ascender, 800);
        assert_eq!(font.hhea.descender, -200);
        assert_eq!(font.head.x_max, 1000);
        assert_eq!(font.family_name.as_deref(), Some("CSSL"));
    }

    #[test]
    fn glyph_index_for_known_codepoint() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).unwrap();
        assert_eq!(font.glyph_index(0x41), 1);
    }

    #[test]
    fn glyph_index_for_unmapped_returns_zero() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).unwrap();
        assert_eq!(font.glyph_index(0x42), 0);
        assert_eq!(font.glyph_index(0x10000), 0);
    }

    #[test]
    fn glyph_outline_returns_three_points_for_synthesized_glyph() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).unwrap();
        let g = font.glyph_outline(1).unwrap();
        assert_eq!(g.contour_end_points, vec![2]);
        assert_eq!(g.points.len(), 3);
        assert_eq!(g.points[0].x, 0);
        assert_eq!(g.points[0].y, 0);
        assert_eq!(g.points[1].x, 1000);
        assert_eq!(g.points[1].y, 0);
        assert_eq!(g.points[2].x, 500);
        assert_eq!(g.points[2].y, 1000);
        assert!(g.points.iter().all(|p| p.on_curve));
    }

    #[test]
    fn glyph_outline_zero_length_glyph_is_empty() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).unwrap();
        let g = font.glyph_outline(0).unwrap();
        assert!(g.contour_end_points.is_empty());
        assert!(g.points.is_empty());
    }

    #[test]
    fn glyph_outline_out_of_range_errors() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).unwrap();
        let r = font.glyph_outline(100);
        assert!(matches!(r, Err(AssetError::InvalidValue { .. })));
    }

    #[test]
    fn glyph_metric_clamps_for_high_indices() {
        let bytes = synthesize_minimal_ttf();
        let font = parse(&bytes).unwrap();
        let m = font.glyph_metric(99);
        assert_eq!(m.advance_width, 1000);
    }

    #[test]
    fn parse_rejects_short_input() {
        let r = parse(&[0u8; 4]);
        assert!(matches!(r, Err(AssetError::Truncated { .. })));
    }

    #[test]
    fn parse_rejects_otto_with_unsupported_message() {
        let mut buf = vec![0u8; 12];
        buf[0..4].copy_from_slice(&SCALER_OTTO);
        let r = parse(&buf);
        assert!(matches!(r, Err(AssetError::UnsupportedKind { .. })));
    }

    #[test]
    fn parse_rejects_bad_scaler() {
        let mut buf = vec![0u8; 12];
        buf[0..4].copy_from_slice(b"BADX");
        let r = parse(&buf);
        assert!(matches!(r, Err(AssetError::BadMagic { .. })));
    }

    #[test]
    fn cmap4_lookup_returns_zero_for_above_u16() {
        let cmap = Cmap4 {
            end_codes: vec![0x41, 0xffff],
            start_codes: vec![0x41, 0xffff],
            id_deltas: vec![0, 1],
            id_range_offsets: vec![0, 0],
            glyph_id_array: vec![],
        };
        assert_eq!(cmap.lookup(0x10000), 0);
    }

    #[test]
    fn cmap4_lookup_segment_with_zero_id_range_offset() {
        let cmap = Cmap4 {
            end_codes: vec![0x42, 0xffff],
            start_codes: vec![0x41, 0xffff],
            id_deltas: vec![10, 1],
            id_range_offsets: vec![0, 0],
            glyph_id_array: vec![],
        };
        // 0x41 + 10 = 0x4b
        assert_eq!(cmap.lookup(0x41), 0x4b);
    }
}

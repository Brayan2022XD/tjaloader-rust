use macroquad::prelude::*;
use resvg::tiny_skia;
use resvg::usvg;
use std::collections::HashMap;

pub struct SpriteTex {
    pub tex: Texture2D,
    pub w: f32,
    pub h: f32,
    pub cx: f32,
    pub cy: f32,
}

pub struct SvgAssets {
    pub stage_bg: Option<SpriteTex>,
    pub don: Option<SpriteTex>,
    pub ka: Option<SpriteTex>,
    pub big_don: Option<SpriteTex>,
    pub big_ka: Option<SpriteTex>,
    pub drumroll_s: Option<SpriteTex>,
    pub drumroll_e: Option<SpriteTex>,
    pub big_drumroll_s: Option<SpriteTex>,
    pub big_drumroll_e: Option<SpriteTex>,
    pub balloon_s: Option<SpriteTex>,
    pub balloon_e: Option<SpriteTex>,
    pub judge_circle: Option<SpriteTex>,
    pub track_red: Option<SpriteTex>,
    pub drum_playfield: Option<SpriteTex>,
    pub drum_icon: Option<SpriteTex>,
    pub don_l: Option<SpriteTex>,
    pub don_r: Option<SpriteTex>,
    pub ka_l: Option<SpriteTex>,
    pub ka_r: Option<SpriteTex>,
    pub diff_kantan: Option<SpriteTex>,
    pub diff_futsuu: Option<SpriteTex>,
    pub diff_muzukashii: Option<SpriteTex>,
    pub diff_oni: Option<SpriteTex>,
    pub diff_ura: Option<SpriteTex>,
    pub judge_bad: Option<SpriteTex>,
    pub judge_ok: Option<SpriteTex>,
    pub judge_good: Option<SpriteTex>,
    pub judge_pgood: Option<SpriteTex>,
    pub burst_ok: Option<SpriteTex>,
    pub burst_good: Option<SpriteTex>,
    pub burst_pgood: Option<SpriteTex>,
    pub ring_ok: Option<SpriteTex>,
    pub ring_good: Option<SpriteTex>,
    pub ring_pgood: Option<SpriteTex>,
    pub lane_don: Option<SpriteTex>,
    pub lane_ka: Option<SpriteTex>,
    pub lane_hit: Option<SpriteTex>,
    pub soul_bg: Option<SpriteTex>,
    pub soul_bg_nh: Option<SpriteTex>,
    pub soul_bg_easy: Option<SpriteTex>,
    pub soul_fill: Option<SpriteTex>,
    pub soul_fill_nh: Option<SpriteTex>,
    pub soul_fill_easy: Option<SpriteTex>,
    pub soul_clear_flash: Option<SpriteTex>,
    pub soul_clear_flash_nh: Option<SpriteTex>,
    pub soul_clear_flash_easy: Option<SpriteTex>,
    pub soul_rainbow: Option<SpriteTex>,
    pub soul_rainbow_nh: Option<SpriteTex>,
    pub soul_rainbow_easy: Option<SpriteTex>,
    pub soul_icon_off: Option<SpriteTex>,
    pub soul_icon_on: Option<SpriteTex>,
    pub combo_label_ja: Option<SpriteTex>,
    pub digits: Vec<Option<SpriteTex>>,
    pub crown_fail: Option<SpriteTex>,
    pub crown_clear: Option<SpriteTex>,
    pub crown_full_gauge: Option<SpriteTex>,
    pub crown_fc: Option<SpriteTex>,
    pub crown_dfc: Option<SpriteTex>,
    pub crown_pdc: Option<SpriteTex>,
    pub res_bar_pgood: Option<SpriteTex>,
    pub res_bar_good: Option<SpriteTex>,
    pub res_bar_ok: Option<SpriteTex>,
    pub res_bar_bad: Option<SpriteTex>,
    pub perc_bg: Option<SpriteTex>,
    pub perc_fill: Option<SpriteTex>,
    pub ranks: HashMap<&'static str, Option<SpriteTex>>,
    pub ex1: Option<SpriteTex>,
    pub ex2: Option<SpriteTex>,
    pub firework_upper: Option<SpriteTex>,
    pub big_glow: Option<SpriteTex>,
    pub roll_flash: Option<SpriteTex>,
    pub balloon_pop_fx: Option<SpriteTex>,
    pub fly_balloon: Option<SpriteTex>,
    pub ghost_miss: Option<SpriteTex>,
    pub roll_bubble: Option<SpriteTex>,
    pub balloon_bubble: Option<SpriteTex>,
    pub confetti: Option<SpriteTex>,
    pub gogo_fire: Vec<Option<SpriteTex>>,
    pub soul_fire: Vec<Option<SpriteTex>>,
}

fn rasterize_svg(
    svg_raw: &str,
    cx: f32,
    cy: f32,
    opt: &usvg::Options,
    font_family_override: Option<&str>,
    supersample: f32,
) -> Option<SpriteTex> {
    let patched;
    let svg_str = if let Some(fam) = font_family_override {
        patched = svg_raw.replace("&quot;KleeOne-Regular&quot;, Sans Serif", fam);
        patched.as_str()
    } else {
        svg_raw
    };

    let tree = usvg::Tree::from_str(svg_str, opt).ok()?;
    let size = tree.size();
    let w = size.width();
    let h = size.height();
    if w <= 0.01 || h <= 0.01 {
        return None;
    }

    let ss = if w <= 1.5 { 1.0 } else { supersample };
    let px_w = (w * ss).ceil().clamp(1.0, 2048.0) as u32;
    let px_h = (h * ss).ceil().clamp(1.0, 2048.0) as u32;
    let sx = (px_w as f32) / w;
    let sy = (px_h as f32) / h;

    let mut pixmap = tiny_skia::Pixmap::new(px_w, px_h)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );

    let mut rgba = Vec::with_capacity(pixmap.pixels().len() * 4);
    for p in pixmap.pixels() {
        let d = p.demultiply();
        rgba.push(d.red());
        rgba.push(d.green());
        rgba.push(d.blue());
        rgba.push(d.alpha());
    }

    let tex = Texture2D::from_rgba8(px_w as u16, px_h as u16, &rgba);
    if w <= 1.5 {
        tex.set_filter(FilterMode::Nearest);
    } else {
        tex.set_filter(FilterMode::Linear);
    }

    Some(SpriteTex { tex, w, h, cx, cy })
}

impl SvgAssets {
    pub fn load(font_bytes: Option<Vec<u8>>) -> Self {
        let mut opt = usvg::Options::default();
        opt.fontdb_mut().load_system_fonts();

        let mut family_override: Option<String> = None;
        if let Some(bytes) = font_bytes {
            let mut tmp_db = usvg::fontdb::Database::new();
            tmp_db.load_font_data(bytes.clone());
            if let Some(face) = tmp_db.faces().next() {
                if let Some((fam, _)) = face.families.first() {
                    family_override = Some(fam.clone());
                    opt.font_family = fam.clone();
                    opt.fontdb_mut().set_sans_serif_family(fam.clone());
                }
            }
            opt.fontdb_mut().load_font_data(bytes);
        }

        let r = |svg: &str, cx: f32, cy: f32| -> Option<SpriteTex> {
            rasterize_svg(svg, cx, cy, &opt, family_override.as_deref(), 2.0)
        };

        let mut ranks = HashMap::new();
        ranks.insert("F", r(include_str!("../svg/b1ba9156b6804dcdf5f4ee59b1d330d4.svg"), 7.5, 18.5));
        ranks.insert("E", r(include_str!("../svg/23a5e1467c360515e54e99b72eb83abf.svg"), 7.88, 18.5));
        ranks.insert("D", r(include_str!("../svg/d38ae8a61217a1309c3a05df33bfcdf5.svg"), 8.78, 18.5));
        ranks.insert("C", r(include_str!("../svg/f500bb6cf1b2b10d60e83c7edf65aee1.svg"), 8.53, 18.5));
        ranks.insert("B", r(include_str!("../svg/57883c10b1b10c4927d233efc56e9df7.svg"), 8.0, 18.5));
        ranks.insert("A", r(include_str!("../svg/43b62ff713f0f0e7e5249280f737d492.svg"), 8.9, 18.5));
        ranks.insert("A+", r(include_str!("../svg/49b346f98d63edb1b785140fd083940b.svg"), 8.9, 18.5));
        ranks.insert("S", r(include_str!("../svg/deaa1a8ce7328c613679ed8f3e2f60ff.svg"), 7.67, 18.5));
        ranks.insert("S+", r(include_str!("../svg/303bbb6cf58c1aa8fd953f6623c4c48c.svg"), 7.67, 18.5));
        ranks.insert("P", r(include_str!("../svg/7eeb462504048d3087f369d25b4d1fee.svg"), 7.5, 18.5));

        Self {
            stage_bg: r(include_str!("../svg/527ffcdb94253d51073f5277c2fea4cf.svg"), 466.5, 279.0),
            don: r(include_str!("../svg/77544a4a0d3cc5150c060d081774ceea.svg"), 16.5, 16.5),
            ka: r(include_str!("../svg/8871ffed90b182eef063d9b36f5c0a37.svg"), 16.5, 16.5),
            big_don: r(include_str!("../svg/5b1072d047b8700273c03f96adc3b61c.svg"), 21.5, 21.5),
            big_ka: r(include_str!("../svg/41b1ed59853acb199eca19e6114f6c4f.svg"), 21.5, 21.5),
            drumroll_s: r(include_str!("../svg/9e0fdb47565a3c0e5710fbd6c636a197.svg"), 16.5, 16.5),
            drumroll_e: r(include_str!("../svg/730eacefb887c244d37d18e0751afab2.svg"), 0.0, 16.5),
            big_drumroll_s: r(include_str!("../svg/70a8780bf5da1589eee58130e4d31f55.svg"), 21.5, 21.5),
            big_drumroll_e: r(include_str!("../svg/d868b74ecb4213bbcdbfed98b2b064a2.svg"), 0.0, 21.5),
            balloon_s: r(include_str!("../svg/4442db38bf3693c9d533b32c4a5c8a9b.svg"), 16.5, 16.5),
            balloon_e: r(include_str!("../svg/5a280c225aa6ff8b6a22d4f042d428f8.svg"), 31.875, 16.5064),
            judge_circle: r(include_str!("../svg/16dd9a3832ba0365e0c7b50a52b187ca.svg"), 21.5, 21.5),
            track_red: r(include_str!("../svg/5b28b6944f9729ed667893ba0f59c70d.svg"), 341.5, 40.0),
            drum_playfield: r(include_str!("../svg/15db0cbe96bd5c4ce44f30aa13513309.svg"), 190.5, -34.5),
            drum_icon: r(include_str!("../svg/95cbaded9ea2d4350935b8e294b5484f.svg"), 59.0, 58.683915),
            don_l: r(include_str!("../svg/2d381bf5855dd05b48ed0bdf33ade2a3.svg"), 52.0, 52.062295),
            don_r: r(include_str!("../svg/1e47d8f5745c63b6c92db1426d7f9a56.svg"), 2.0, 52.062292),
            ka_l: r(include_str!("../svg/2aa952e81daca5a7c158f40d51e9674f.svg"), 59.0, 59.062292),
            ka_r: r(include_str!("../svg/b3371a599f86b589e5d632aedf466a2d.svg"), 2.0, 59.062293),
            diff_kantan: r(include_str!("../svg/81a39064d7aee9d5dc41267c1ed0f1bb.svg"), 20.203876, 20.203876),
            diff_futsuu: r(include_str!("../svg/79c781d103b0acbd54dd132412049c27.svg"), 16.05215, 20.368003),
            diff_muzukashii: r(include_str!("../svg/b61b22d3743a501b2401625c49c32f9b.svg"), 14.040453, 20.230612),
            diff_oni: r(include_str!("../svg/1fb91bb87dbd8292e8dcab55b365f2de.svg"), 19.871987, 18.782124),
            diff_ura: r(include_str!("../svg/75b06a6fb62ea324d224b3a3ab7b51a0.svg"), 19.871989, 18.782122),
            judge_bad: r(include_str!("../svg/1859187be4babb640ed6d8c612896b90.svg"), 22.0, 19.0),
            judge_ok: r(include_str!("../svg/73ac8856b13a48f5804fd66b0710d7ea.svg"), 12.0, 19.0),
            judge_good: r(include_str!("../svg/79924f3a4ce5f62951e49004316dbc09.svg"), 12.0, 19.0),
            judge_pgood: r(include_str!("../svg/0c605359f08aee369f1178c8acd55a33.svg"), 12.0, 19.0),
            burst_ok: r(include_str!("../svg/67482970b347ad624aa02d81d0be51af.svg"), 37.5, 37.5),
            burst_good: r(include_str!("../svg/ab30f74133ea3a5b72483dabc31b1e80.svg"), 45.0, 45.0),
            burst_pgood: r(include_str!("../svg/c405002f5b0f4ff6ab97d541748b5538.svg"), 52.5, 52.5),
            ring_ok: r(include_str!("../svg/c16d3a9c02d3a5f72f1e83cf4c13112d.svg"), 16.5, 16.5),
            ring_good: r(include_str!("../svg/5bc7623a20b39ca469e0502d838915fb.svg"), 16.5, 16.5),
            ring_pgood: r(include_str!("../svg/6747c9dc0e573f9e2f92c0203dc9bd19.svg"), 16.5, 16.375),
            lane_don: r(include_str!("../svg/335fe5eaf3fdc64ee1693cf2684bfcaa.svg"), 188.5, 55.0),
            lane_ka: r(include_str!("../svg/4696c25a195619cc2461b441cc9af439.svg"), 188.5, 55.0),
            lane_hit: r(include_str!("../svg/34d299099fade3aac85248c3a2a37c46.svg"), 188.5, 55.0),
            soul_bg: r(include_str!("../svg/c15687892391b10851cd0869582f28a8.svg"), 97.5, 10.0),
            soul_bg_nh: r(include_str!("../svg/cead1f7650e67d1e90ad0bbb85721d48.svg"), 97.5, 10.0),
            soul_bg_easy: r(include_str!("../svg/8456672711f675a23b5584e76ce396f5.svg"), 97.5, 10.0),
            soul_fill: r(include_str!("../svg/8f16b345ad6f7ff0ecadbd2d2e951d97.svg"), 97.5, 10.0),
            soul_fill_nh: r(include_str!("../svg/25b86fb5f79b14ee1ee9e83793ab9770.svg"), 97.5, 10.0),
            soul_fill_easy: r(include_str!("../svg/8a72cb5354b55995e76f26bea9f15b5f.svg"), 97.5, 10.0),
            soul_clear_flash: r(include_str!("../svg/9fa04042aed85b78747494088ca31bcc.svg"), 97.5, 5.0),
            soul_clear_flash_nh: r(include_str!("../svg/0b4f27f080370dce44091b2160e72626.svg"), 97.5, 5.0),
            soul_clear_flash_easy: r(include_str!("../svg/e61c79df31607a2c26a195111fd0dc98.svg"), 97.5, 5.0),
            soul_rainbow: r(include_str!("../svg/420d05b42a653f196b82331384061fce.svg"), 97.5, 10.0),
            soul_rainbow_nh: r(include_str!("../svg/c7c06e43362acb2c7adf94c60b3d3ada.svg"), 97.5, 10.0),
            soul_rainbow_easy: r(include_str!("../svg/9ee66680a2396cee65a502d35bc7d837.svg"), 97.5, 10.0),
            soul_icon_off: r(include_str!("../svg/c191d27003653990e58723710706961d.svg"), 13.70825, 21.87487),
            soul_icon_on: r(include_str!("../svg/de3751a9d44be2d933c0cc560b87a870.svg"), 13.70825, 21.87487),
            combo_label_ja: r(include_str!("../svg/506d5d8c4f1c261ec2bd5de8b4131a1e.svg"), 33.25, 20.25),
            digits: vec![
                r(include_str!("../svg/86a86688b1a7c2837ea173c20ae18630.svg"), 8.0, 19.0),
                r(include_str!("../svg/725baa9277b7a58f9356027226ca8e22.svg"), 8.0, 19.0),
                r(include_str!("../svg/dab339c3cdb739692015f09a3fd49686.svg"), 8.0, 19.0),
                r(include_str!("../svg/87ab6c9faf0b18893d938324379bdbd9.svg"), 8.0, 19.0),
                r(include_str!("../svg/4df9a1bf2ff1a061fac40b255f5e69cf.svg"), 8.0, 19.0),
                r(include_str!("../svg/96e8c482ec306c3bb2c5cbdc7464029c.svg"), 8.0, 19.0),
                r(include_str!("../svg/35c9e1f08b580d3b789166d6da38964d.svg"), 8.0, 19.0),
                r(include_str!("../svg/d4549c0bc6c0e0b038ff0f6c20630e45.svg"), 8.0, 19.0),
                r(include_str!("../svg/684cc3856feadc1c18bfc2027e13cc03.svg"), 8.0, 19.0),
                r(include_str!("../svg/6520d8e0c406f713262cdf723a4e091b.svg"), 8.0, 19.0),
            ],
            crown_fail: r(include_str!("../svg/1fcf96ec2403ea66027658e99a668fa6.svg"), 162.0, 117.0),
            crown_clear: r(include_str!("../svg/949941bea38a2bbb16304c44256c9afd.svg"), 162.0, 117.0),
            crown_full_gauge: r(include_str!("../svg/e5aa681d5c2f49049f535f713206363f.svg"), 162.0, 117.0),
            crown_fc: r(include_str!("../svg/8f4c8ff3279afedc874caa68c9688a64.svg"), 162.0, 117.0),
            crown_dfc: r(include_str!("../svg/54043c4399583af4b8bbf2f60280526e.svg"), 162.0, 117.0),
            crown_pdc: r(include_str!("../svg/bf7d11f7e14fea418f8c5f38646b6554.svg"), 162.0, 117.0),
            res_bar_pgood: r(include_str!("../svg/b87b04b298ce5e8fc436b90691b0f7db.svg"), 0.0, 5.0),
            res_bar_good: r(include_str!("../svg/53a67f662b943a4041d9a3e28432cc4b.svg"), 0.0, 5.0),
            res_bar_ok: r(include_str!("../svg/0d6a628487e5e41ef577bb0aeba83ef0.svg"), 0.0, 5.0),
            res_bar_bad: r(include_str!("../svg/cb8e84bc323e7e83cbee8ce0e3dc3b30.svg"), 0.0, 5.0),
            perc_bg: r(include_str!("../svg/c381d5a3d8ba2f554ff37aadc66f64cf.svg"), 0.0, 5.0),
            perc_fill: r(include_str!("../svg/4ee375a9bcba3c627efc3e845f95eb85.svg"), 0.0, 5.0),
            ranks,
            ex1: r(include_str!("../svg/879147b9d7237c0addc28d7f5b0d324b.svg"), 95.657145, 95.657145),
            ex2: r(include_str!("../svg/7d7e68f2fa9eadc36c7191bc55563b53.svg"), 111.80909, 111.80909),
            firework_upper: r(include_str!("../svg/74de80889c6c9620262f501d95411d6b.svg"), 20.0, 20.0),
            big_glow: r(include_str!("../svg/58de80daa0aa316c1df76aed268bbc11.svg"), 39.4, 39.4),
            roll_flash: r(include_str!("../svg/e88a02c4fe53d6c011f63fecc703ce01.svg"), 30.0, 30.0),
            balloon_pop_fx: r(include_str!("../svg/4659fac7e6dd16be0ca8967a38bc5886.svg"), 60.0, 60.0),
            fly_balloon: r(include_str!("../svg/68e5b82f5e5dd8df6ed592128ab5accc.svg"), 63.916655, 45.166665),
            ghost_miss: r(include_str!("../svg/dde7a60342b1353d5c7f14616bb5d27a.svg"), 45.84061, 24.069446),
            roll_bubble: r(include_str!("../svg/b859d350877164886fc0491b0c1aa7c0.svg"), 85.0678, 46.65831),
            balloon_bubble: r(include_str!("../svg/f0cdb99d923c2e1a430debd57812ec36.svg"), 34.03169, 22.99365),
            confetti: r(include_str!("../svg/f4194190e5551f6c3ecbbbdc376a7f88.svg"), 26.25, 4.375),
            gogo_fire: vec![
                r(include_str!("../svg/deee7d11d9d5e2e88c61255f64d9d971.svg"), 27.5, 44.47126),
                r(include_str!("../svg/4937dda75b68058391829c151742ddec.svg"), 27.5, 47.47126),
                r(include_str!("../svg/db73c0f90ef757fa9f9adb43079ae397.svg"), 27.5, 43.78309),
                r(include_str!("../svg/0fbfeb10d83a892dbe6e835d98489fee.svg"), 27.5, 40.29963),
                r(include_str!("../svg/c33078fc784ac770e67106b7c7af81c8.svg"), 27.5, 38.2802),
                r(include_str!("../svg/a0724a22a2f573a351adc583bc27227d.svg"), 27.5, 43.41269),
            ],
            soul_fire: vec![
                r(include_str!("../svg/dd1e0d620301ab135f8b8932c46a2314.svg"), 27.5, 44.47126),
                r(include_str!("../svg/4937dda75b68058391829c151742ddec.svg"), 27.5, 47.47126),
                r(include_str!("../svg/999d587fce8c41166fe3d34e70d8eeed.svg"), 27.5, 43.78309),
                r(include_str!("../svg/0fbfeb10d83a892dbe6e835d98489fee.svg"), 27.5, 40.29963),
                r(include_str!("../svg/4f56f66b0bea6c0a0f8f831f719cbbf1.svg"), 27.5, 38.2802),
                r(include_str!("../svg/a0724a22a2f573a351adc583bc27227d.svg"), 27.5, 43.41269),
            ],
        }
    }
}
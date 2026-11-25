mod assets_svg;

use assets_svg::{SpriteTex, SvgAssets};
use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams};
use macroquad::prelude::*;
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::collections::HashMap;

use std::fs;

use std::io::Cursor;

use std::path::{Path, PathBuf};

use std::sync::Arc;

use std::time::{Duration, Instant};



const STAGE_W: f32 = 640.0;

const STAGE_H: f32 = 360.0;

const X_JUEZ: f64 = -150.0;

const Y_PISTA: f32 = 25.0;

const GLOBO_OFFSET: f64 = 31.875;

const SOUL_X: f32 = 297.5;

const SOUL_Y: f32 = 64.0;



// ============================================================================

// SHADERS PARA EFECTOS DE SCRATCH (HUE SHIFT, BRIGHTNESS Y BLEND ADITIVO)

// ============================================================================

const VERTEX_SHADER: &str = r#"#version 100

attribute vec3 position;

attribute vec2 texcoord;

attribute vec4 color0;



varying lowp vec2 uv;

varying lowp vec4 color;



uniform mat4 Model;

uniform mat4 Projection;



void main() {

    gl_Position = Projection * Model * vec4(position, 1);

    color = color0 / 255.0;

    uv = texcoord;

}

"#;



const FRAGMENT_SHADER: &str = r#"#version 100

precision mediump float;



varying lowp vec4 color;

varying lowp vec2 uv;



uniform sampler2D Texture;

uniform float u_hue_shift;

uniform float u_brightness;



vec3 rgb2hsv(vec3 c) {

    vec4 K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);

    vec4 p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));

    vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));

    float d = q.x - min(q.w, q.y);

    float e = 1.0e-10;

    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);

}



vec3 hsv2rgb(vec3 c) {

    vec4 K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);

    vec3 p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);

    return c.z * mix(K.xxx, clamp(p - K.xxx, 0.0, 1.0), c.y);

}



void main() {

    vec4 tex_col = texture2D(Texture, uv);

    vec3 rgb = tex_col.rgb * color.rgb;

    if (u_hue_shift != 0.0) {

        vec3 hsv = rgb2hsv(rgb);

        hsv.x = fract(hsv.x + u_hue_shift / 360.0);

        rgb = hsv2rgb(hsv);

    }

    if (u_brightness > 0.0) {

        rgb = mix(rgb, vec3(1.0), clamp(u_brightness, 0.0, 1.0));

    }

    gl_FragColor = vec4(rgb, tex_col.a * color.a);

}

"#;



struct RenderMaterials {

    normal_fx: Material,

    additive_fx: Material,

}



impl RenderMaterials {

    fn new() -> Self {

        let normal_fx = load_material(

            ShaderSource::Glsl {

                vertex: VERTEX_SHADER,

                fragment: FRAGMENT_SHADER,

            },

            MaterialParams {

                pipeline_params: PipelineParams {

                    color_blend: Some(BlendState::new(

                        Equation::Add,

                        BlendFactor::Value(BlendValue::SourceAlpha),

                        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),

                    )),

                    ..Default::default()

                },

                uniforms: vec![

                    UniformDesc::new("u_hue_shift", UniformType::Float1),

                    UniformDesc::new("u_brightness", UniformType::Float1),

                ],

                ..Default::default()

            },

        )

        .expect("No se pudo compilar el shader normal");



        let additive_fx = load_material(

            ShaderSource::Glsl {

                vertex: VERTEX_SHADER,

                fragment: FRAGMENT_SHADER,

            },

            MaterialParams {

                pipeline_params: PipelineParams {

                    color_blend: Some(BlendState::new(

                        Equation::Add,

                        BlendFactor::Value(BlendValue::SourceAlpha),

                        BlendFactor::One,

                    )),

                    ..Default::default()

                },

                uniforms: vec![

                    UniformDesc::new("u_hue_shift", UniformType::Float1),

                    UniformDesc::new("u_brightness", UniformType::Float1),

                ],

                ..Default::default()

            },

        )

        .expect("No se pudo compilar el shader aditivo");



        Self {

            normal_fx,

            additive_fx,

        }

    }

}



#[derive(Clone)]

struct SharedBytes(Arc<Vec<u8>>);



impl AsRef<[u8]> for SharedBytes {

    fn as_ref(&self) -> &[u8] {

        &self.0

    }

}



// ============================================================================

// DIFICULTADES Y PARSER TJA

// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]

enum Course {

    Kantan,

    Futsuu,

    Muzukashii,

    Oni,

    UraOni,

}



impl Course {

    const ALL: [Course; 5] = [

        Course::Kantan,

        Course::Futsuu,

        Course::Muzukashii,

        Course::Oni,

        Course::UraOni,

    ];



    fn from_tja(raw: &str) -> Self {

        let c = raw.trim().to_lowercase();

        match c.as_str() {

            "0" | "easy" | "kantan" | "かんたん" => Course::Kantan,

            "1" | "normal" | "futsuu" | "ふつう" => Course::Futsuu,

            "2" | "hard" | "muzukashii" | "むずかしい" => Course::Muzukashii,

            "3" | "oni" | "おに" => Course::Oni,

            "4" | "edit" | "ura" | "ura oni" | "おに (裏)" | "裏" => Course::UraOni,

            _ => Course::Oni,

        }

    }



    fn name(&self) -> &'static str {

        match self {

            Course::Kantan => "Kantan",

            Course::Futsuu => "Futsuu",

            Course::Muzukashii => "Muzukashii",

            Course::Oni => "Oni",

            Course::UraOni => "Ura Oni",

        }

    }



    fn key_hint(&self) -> &'static str {

        match self {

            Course::Kantan => "1",

            Course::Futsuu => "2",

            Course::Muzukashii => "3",

            Course::Oni => "4",

            Course::UraOni => "5",

        }

    }



    fn color(&self) -> Color {

        match self {

            Course::Kantan => Color::from_rgba(255, 85, 65, 255),

            Course::Futsuu => Color::from_rgba(105, 195, 65, 255),

            Course::Muzukashii => Color::from_rgba(55, 155, 235, 255),

            Course::Oni => Color::from_rgba(215, 55, 145, 255),

            Course::UraOni => Color::from_rgba(145, 65, 220, 255),

        }

    }



    fn is_easy_or_normal(&self) -> bool {

        matches!(self, Course::Kantan | Course::Futsuu)

    }



    fn combo_color_threshold(&self) -> u32 {

        match self {

            Course::Kantan => 30,

            Course::Futsuu | Course::Muzukashii => 50,

            Course::Oni | Course::UraOni => 100,

        }

    }

}



#[derive(Clone, Debug, Default)]

struct CourseData {

    exists: bool,

    level: f64,

    balloons: Vec<i32>,

    measures: Vec<String>,

    note_count: u32,

}



#[derive(Clone, Debug)]

struct TjaData {

    title: String,

    bpm: f64,

    offset: f64,

    wave: String,

    demo_start: f64,

    song_vol: f32,

    se_vol: f32,

    courses: HashMap<Course, CourseData>,

    tja_dir: Option<PathBuf>,

}



impl Default for TjaData {

    fn default() -> Self {

        let mut courses = HashMap::new();

        for c in Course::ALL {

            courses.insert(c, CourseData::default());

        }

        Self {

            title: "Sin Título".to_string(),

            bpm: 120.0,

            offset: 0.0,

            wave: String::new(),

            demo_start: 0.0,

            song_vol: 100.0,

            se_vol: 100.0,

            courses,

            tja_dir: None,

        }

    }

}



fn decode_tja_bytes(bytes: &[u8]) -> String {

    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {

        return String::from_utf8_lossy(&bytes[3..]).into_owned();

    }

    if let Ok(s) = std::str::from_utf8(bytes) {

        return s.to_string();

    }

    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);

    cow.into_owned()

}



fn strip_comment(line: &str) -> &str {

    match line.find("//") {

        Some(idx) => &line[..idx],

        None => line,

    }

}



fn parse_tja(text: &str, tja_dir: Option<PathBuf>) -> TjaData {

    let mut tja = TjaData {

        tja_dir,

        ..Default::default()

    };



    let mut is_reading_notes = false;

    let mut current_course = Course::Oni;

    let mut current_measures: Vec<String> = Vec::new();

    let mut current_measure_lines: Vec<String> = Vec::new();

    let mut global_balloons: Vec<i32> = Vec::new();



    for raw_line in text.lines() {

        let line = raw_line.trim();

        if line.starts_with("//") || line.is_empty() {

            continue;

        }

        let upper = line.to_uppercase();



        if !is_reading_notes {

            if upper.starts_with("TITLE:") {

                tja.title = line[6..].trim().to_string();

            } else if upper.starts_with("BPM:") {

                if let Ok(v) = line[4..].trim().parse::<f64>() {

                    if v > 0.0 {

                        tja.bpm = v;

                    }

                }

            } else if upper.starts_with("OFFSET:") {

                if let Ok(v) = line[7..].trim().parse::<f64>() {

                    tja.offset = v;

                }

            } else if upper.starts_with("WAVE:") {

                tja.wave = line[5..].trim().trim_matches('"').to_string();

            } else if upper.starts_with("DEMOSTART:") {

                if let Ok(v) = line[10..].trim().parse::<f64>() {

                    tja.demo_start = v.max(0.0);

                }

            } else if upper.starts_with("SONGVOL:") {

                if let Ok(v) = line[8..].trim().parse::<f32>() {

                    tja.song_vol = v.clamp(0.0, 200.0);

                }

            } else if upper.starts_with("SEVOL:") {

                if let Ok(v) = line[6..].trim().parse::<f32>() {

                    tja.se_vol = v.clamp(0.0, 200.0);

                }

            } else if upper.starts_with("COURSE:") {

                current_course = Course::from_tja(&line[7..]);

                if let Some(c) = tja.courses.get_mut(&current_course) {

                    c.exists = true;

                    if c.balloons.is_empty() && !global_balloons.is_empty() {

                        c.balloons = global_balloons.clone();

                    }

                }

            } else if upper.starts_with("LEVEL:") {

                if let Ok(v) = line[6..].trim().parse::<f64>() {

                    if let Some(c) = tja.courses.get_mut(&current_course) {

                        c.level = v;

                    }

                }

            } else if upper.starts_with("BALLOON:") {

                let parsed: Vec<i32> = line[8..]

                    .split(',')

                    .filter_map(|s| s.trim().parse::<i32>().ok())

                    .collect();

                global_balloons = parsed.clone();

                if let Some(c) = tja.courses.get_mut(&current_course) {

                    c.balloons = parsed;

                }

            } else if upper == "#START" || upper.starts_with("#START ") {

                is_reading_notes = true;

                current_measures.clear();

                current_measure_lines.clear();

                if let Some(c) = tja.courses.get_mut(&current_course) {

                    c.exists = true;

                }

            }

        } else if upper == "#END" {

            is_reading_notes = false;

            if !current_measure_lines.is_empty() {

                let leftover = current_measure_lines.join("\n").trim().to_string();

                if !leftover.is_empty() {

                    current_measures.push(leftover);

                }

            }

            if let Some(c) = tja.courses.get_mut(&current_course) {

                c.measures = current_measures.clone();

                c.note_count = count_notes_in_measures(&c.measures);

            }

        } else if upper.starts_with("#LYRIC") || line.starts_with('#') {

            current_measure_lines.push(line.to_string());

        } else {

            let parts: Vec<&str> = line.split(',').collect();

            for (i, part) in parts.iter().enumerate() {

                current_measure_lines.push(part.to_string());

                if i + 1 < parts.len() {

                    let joined = current_measure_lines.join("\n").trim().to_string();

                    current_measures.push(joined);

                    current_measure_lines.clear();

                }

            }

        }

    }



    tja

}



fn count_notes_in_measures(measures: &[String]) -> u32 {

    let mut total = 0;

    for measure in measures {

        for line in measure.lines() {

            let clean = strip_comment(line).trim();

            if !clean.starts_with('#') {

                for ch in clean.chars() {

                    if matches!(ch, '1' | '2' | '3' | '4') {

                        total += 1;

                    }

                }

            }

        }

    }

    total

}



// ============================================================================

// CARGA DE ASSETS MULTIPLATAFORMA (PC + Android APK AssetManager)

// ============================================================================

#[cfg(not(target_os = "android"))]

fn find_asset_fs(filename: &str) -> Option<PathBuf> {

    let mut search_dirs = vec![PathBuf::from("."), PathBuf::from("assets")];

    if let Ok(exe) = std::env::current_exe() {

        if let Some(dir) = exe.parent() {

            search_dirs.push(dir.to_path_buf());

            search_dirs.push(dir.join("assets"));

        }

    }



    let target_lower = filename.to_lowercase();

    for dir in &search_dirs {

        let exact = dir.join(filename);

        if exact.exists() {

            return Some(exact);

        }

        if let Ok(entries) = fs::read_dir(dir) {

            for entry in entries.flatten() {

                if let Some(name) = entry.file_name().to_str() {

                    if name.to_lowercase() == target_lower {

                        return Some(entry.path());

                    }

                }

            }

        }

    }

    None

}



async fn load_asset_bytes(filename: &str) -> Option<Vec<u8>> {

    #[cfg(not(target_os = "android"))]

    {

        if let Some(path) = find_asset_fs(filename) {

            if let Ok(bytes) = fs::read(&path) {

                return Some(bytes);

            }

        }

    }

    if let Ok(bytes) = macroquad::file::load_file(filename).await {

        return Some(bytes);

    }

    let in_assets = format!("assets/{}", filename);

    macroquad::file::load_file(&in_assets).await.ok()

}



// ============================================================================

// EXPLORADOR DE ARCHIVOS INTEGRADO (Para Android)

// ============================================================================

#[allow(dead_code)]

const ANDROID_APP_FILES_DIR: &str = "/storage/emulated/0/Android/data/com.taiko.tjaloader/files";



#[derive(Clone, Copy, PartialEq, Eq)]

#[allow(dead_code)]

enum BrowserMode {

    PickTja,

    PickAudio,

}



#[derive(Clone, Copy, PartialEq, Eq)]

#[allow(dead_code)]

enum BrowserReturn {

    NeedFile,

    SelectDifficulty,

}



#[allow(dead_code)]

struct BrowserEntry {

    name: String,

    path: PathBuf,

    is_dir: bool,

}



#[allow(dead_code)]

struct FileBrowserState {

    mode: BrowserMode,

    return_to: BrowserReturn,

    current_dir: PathBuf,

    entries: Vec<BrowserEntry>,

    scroll_y: f32,

    error_msg: Option<String>,

    drag_active: bool,

    drag_start_y: f32,

    last_touch_y: f32,

    drag_moved: bool,

}



#[allow(dead_code)]

impl FileBrowserState {

    fn new(mode: BrowserMode, return_to: BrowserReturn, start_dir: Option<PathBuf>) -> Self {

        let dir = start_dir.unwrap_or_else(|| {

            let candidates = [

                PathBuf::from("/storage/emulated/0/TaikoTJA"),

                PathBuf::from(ANDROID_APP_FILES_DIR),

                PathBuf::from("/storage/emulated/0/Download"),

                PathBuf::from("/storage/emulated/0"),

            ];

            for c in &candidates {

                if c.exists() && fs::read_dir(c).is_ok() {

                    return c.clone();

                }

            }

            PathBuf::from(ANDROID_APP_FILES_DIR)

        });

        let mut st = Self {

            mode,

            return_to,

            current_dir: dir,

            entries: Vec::new(),

            scroll_y: 0.0,

            error_msg: None,

            drag_active: false,

            drag_start_y: 0.0,

            last_touch_y: 0.0,

            drag_moved: false,

        };

        st.refresh();

        st

    }



    fn is_valid_file(&self, path: &Path) -> bool {

        let ext = path

            .extension()

            .and_then(|s| s.to_str())

            .map(|s| s.to_lowercase());

        match self.mode {

            BrowserMode::PickTja => ext.as_deref() == Some("tja"),

            BrowserMode::PickAudio => matches!(

                ext.as_deref(),

                Some("ogg") | Some("mp3") | Some("wav") | Some("flac") | Some("m4a")

            ),

        }

    }



    fn refresh(&mut self) {

        self.entries.clear();

        self.scroll_y = 0.0;

        self.error_msg = None;



        if let Some(parent) = self.current_dir.parent() {

            self.entries.push(BrowserEntry {

                name: ".. (Subir carpeta)".to_string(),

                path: parent.to_path_buf(),

                is_dir: true,

            });

        }



        match fs::read_dir(&self.current_dir) {

            Ok(rd) => {

                let mut dirs = Vec::new();

                let mut files = Vec::new();

                for entry in rd.flatten() {

                    let path = entry.path();

                    let name = entry.file_name().to_string_lossy().into_owned();

                    if name.starts_with('.') {

                        continue;

                    }

                    if path.is_dir() {

                        dirs.push(BrowserEntry {

                            name,

                            path,

                            is_dir: true,

                        });

                    } else if self.is_valid_file(&path) {

                        files.push(BrowserEntry {

                            name,

                            path,

                            is_dir: false,

                        });

                    }

                }

                dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

                self.entries.extend(dirs);

                self.entries.extend(files);



                if self.entries.len() <= 1 {

                    self.error_msg = Some(match self.mode {

                        BrowserMode::PickTja => format!(

                            "Carpeta vacía de .tja. Copia tus canciones en:\n{}",

                            ANDROID_APP_FILES_DIR

                        ),

                        BrowserMode::PickAudio => {

                            "No hay audios (.ogg/.mp3/.wav) en esta carpeta.".to_string()

                        }

                    });

                }

            }

            Err(e) => {

                self.error_msg = Some(format!(

                    "Sin acceso ({}). Usa 'AppData' o da permiso de archivos en Ajustes de Android.",

                    e

                ));

            }

        }

    }

}



// ============================================================================

// MOTOR DE AUDIO (rodio)

// ============================================================================

struct CachedSfx {

    channels: u16,

    sample_rate: u32,

    samples: Arc<Vec<f32>>,

}



impl CachedSfx {

    fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {

        let decoder = Decoder::new(Cursor::new(SharedBytes(Arc::new(bytes))))

            .map_err(|e| format!("Error decodificando SFX: {}", e))?;

        let channels = decoder.channels();

        let sample_rate = decoder.sample_rate();

        let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

        Ok(Self {

            channels,

            sample_rate,

            samples: Arc::new(samples),

        })

    }



    fn to_source(&self) -> SamplesBuffer<f32> {

        SamplesBuffer::new(self.channels, self.sample_rate, self.samples.as_ref().clone())

    }

}



fn find_tja_audio(tja: &TjaData) -> Option<PathBuf> {

    let dir = tja.tja_dir.as_ref()?;

    if !tja.wave.is_empty() {

        let exact = dir.join(&tja.wave);

        if exact.exists() {

            return Some(exact);

        }



        let wave_path = Path::new(&tja.wave);

        let wave_name_lower = wave_path

            .file_name()

            .and_then(|s| s.to_str())

            .map(|s| s.to_lowercase());

        let wave_stem_lower = wave_path

            .file_stem()

            .and_then(|s| s.to_str())

            .map(|s| s.to_lowercase());



        if let Ok(entries) = fs::read_dir(dir) {

            let files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();

            if let Some(ref wname) = wave_name_lower {

                for f in &files {

                    if let Some(fname) = f.file_name().and_then(|s| s.to_str()) {

                        if fname.to_lowercase() == *wname {

                            return Some(f.clone());

                        }

                    }

                }

            }

            if let Some(ref wstem) = wave_stem_lower {

                for ext in &["ogg", "mp3", "wav", "flac", "m4a"] {

                    for f in &files {

                        let fstem = f.file_stem().and_then(|s| s.to_str()).map(|s| s.to_lowercase());

                        let fext = f.extension().and_then(|s| s.to_str()).map(|s| s.to_lowercase());

                        if fstem.as_deref() == Some(wstem.as_str()) && fext.as_deref() == Some(*ext) {

                            return Some(f.clone());

                        }

                    }

                }

            }

        }

    }

    None

}



struct AudioEngine {

    _stream: OutputStream,

    handle: OutputStreamHandle,

    sfx_don: Option<CachedSfx>,

    sfx_ka: Option<CachedSfx>,

    sfx_balloon: Option<CachedSfx>,

    sfx_don_sink: Option<Sink>,

    sfx_ka_sink: Option<Sink>,

    sfx_balloon_sink: Option<Sink>,

    sfx_status: String,

    song_name: String,

    song_bytes: Option<SharedBytes>,

    song_sink: Option<Sink>,

    last_audio_error: Option<String>,

    last_raw_pos: f64,

    last_pos_change_instant: Option<Instant>,

    smoothed_time: f64,

}



impl AudioEngine {

    fn new(

        don_bytes: Option<Vec<u8>>,

        ka_bytes: Option<Vec<u8>>,

        balloon_bytes: Option<Vec<u8>>,

    ) -> Result<Self, String> {

        let (stream, handle) = OutputStream::try_default()

            .map_err(|e| format!("No se pudo inicializar el dispositivo de audio: {}", e))?;



        let sfx_don = don_bytes.and_then(|b| CachedSfx::from_bytes(b).ok());

        let sfx_ka = ka_bytes.and_then(|b| CachedSfx::from_bytes(b).ok());

        let sfx_balloon = balloon_bytes.and_then(|b| CachedSfx::from_bytes(b).ok());



        let sfx_status = format!(

            "SFX: don={} ka={} balloon={}",

            if sfx_don.is_some() { "OK" } else { "NO" },

            if sfx_ka.is_some() { "OK" } else { "NO" },

            if sfx_balloon.is_some() { "OK" } else { "NO" }

        );



        Ok(Self {

            _stream: stream,

            handle,

            sfx_don,

            sfx_ka,

            sfx_balloon,

            sfx_don_sink: None,

            sfx_ka_sink: None,

            sfx_balloon_sink: None,

            sfx_status,

            song_name: "Ninguno".to_string(),

            song_bytes: None,

            song_sink: None,

            last_audio_error: None,

            last_raw_pos: 0.0,

            last_pos_change_instant: None,

            smoothed_time: 0.0,

        })

    }



    fn play_sfx_exclusive(

        handle: &OutputStreamHandle,

        sfx: &Option<CachedSfx>,

        slot: &mut Option<Sink>,

        vol_percent: f32,

    ) {

        if let Some(prev) = slot.take() {

            prev.stop();

        }

        if let Some(data) = sfx {

            if let Ok(sink) = Sink::try_new(handle) {

                sink.set_volume((vol_percent / 100.0).clamp(0.0, 2.0));

                sink.append(data.to_source());

                sink.play();

                *slot = Some(sink);

            }

        }

    }



    fn play_don(&mut self, se_vol: f32) {

        Self::play_sfx_exclusive(&self.handle, &self.sfx_don, &mut self.sfx_don_sink, se_vol);

    }



    fn play_ka(&mut self, se_vol: f32) {

        Self::play_sfx_exclusive(&self.handle, &self.sfx_ka, &mut self.sfx_ka_sink, se_vol);

    }



    fn play_balloon(&mut self, se_vol: f32) {

        Self::play_sfx_exclusive(

            &self.handle,

            &self.sfx_balloon,

            &mut self.sfx_balloon_sink,

            se_vol,

        );

    }



    fn load_song_bytes(&mut self, name: String, bytes: Vec<u8>) -> Result<(), String> {

        self.stop_song();

        self.last_audio_error = None;

        let shared = SharedBytes(Arc::new(bytes));

        Decoder::new(Cursor::new(shared.clone()))

            .map_err(|e| format!("Formato de audio no soportado o dañado: {}", e))?;



        self.song_name = name;

        self.song_bytes = Some(shared);

        Ok(())

    }



    fn load_song_from_path(&mut self, path: &Path) -> Result<(), String> {

        let bytes = fs::read(path)

            .map_err(|e| format!("No se pudo leer {:?}: {}", path.file_name().unwrap_or_default(), e))?;

        let name = path

            .file_name()

            .and_then(|s| s.to_str())

            .unwrap_or("audio")

            .to_string();

        self.load_song_bytes(name, bytes)

    }



    fn clear_song(&mut self) {

        self.stop_song();

        self.song_bytes = None;

        self.song_name = "Ninguno".to_string();

        self.last_audio_error = None;

    }



    fn start_song(&mut self, at_seconds: f64, song_vol: f32) -> Result<(), String> {

        self.stop_song();

        self.last_audio_error = None;



        let bytes = self

            .song_bytes

            .as_ref()

            .cloned()

            .ok_or_else(|| "No hay archivo de audio cargado".to_string())?;



        let decoder = Decoder::new(Cursor::new(bytes.clone()))

            .map_err(|e| format!("Error decodificando audio: {}", e))?;



        let mut sink =

            Sink::try_new(&self.handle).map_err(|e| format!("Error creando Sink: {}", e))?;

        sink.set_volume((song_vol / 100.0).clamp(0.0, 2.0));

        sink.append(decoder);



        let mut actual_start = 0.0;

        if at_seconds > 0.05 {

            if sink.try_seek(Duration::from_secs_f64(at_seconds)).is_ok() {

                actual_start = at_seconds;

            } else {

                sink.stop();

                sink = Sink::try_new(&self.handle)

                    .map_err(|e| format!("Error creando Sink: {}", e))?;

                sink.set_volume((song_vol / 100.0).clamp(0.0, 2.0));

                if let Ok(dec2) = Decoder::new(Cursor::new(bytes)) {

                    sink.append(dec2.skip_duration(Duration::from_secs_f64(at_seconds)));

                    actual_start = at_seconds;

                }

            }

        }

        sink.play();



        self.last_raw_pos = actual_start;

        self.last_pos_change_instant = None;

        self.smoothed_time = actual_start;

        self.song_sink = Some(sink);

        Ok(())

    }



    fn stop_song(&mut self) {

        if let Some(sink) = self.song_sink.take() {

            sink.stop();

        }

        self.last_pos_change_instant = None;

        self.smoothed_time = 0.0;

    }



    fn is_playing(&self) -> bool {

        match &self.song_sink {

            Some(sink) => !sink.empty(),

            None => false,

        }

    }



    fn update_and_get_time(&mut self) -> f64 {

        if let Some(sink) = &self.song_sink {

            let raw = sink.get_pos().as_secs_f64();

            let now = Instant::now();



            if (raw - self.last_raw_pos).abs() > 0.0005 {

                self.last_raw_pos = raw;

                self.last_pos_change_instant = Some(now);

            }



            let est = if let Some(inst) = self.last_pos_change_instant {

                self.last_raw_pos + inst.elapsed().as_secs_f64()

            } else {

                raw

            };



            if (est - raw).abs() > 0.04 {

                self.smoothed_time = raw;

                self.last_raw_pos = raw;

                if raw > 0.0 {

                    self.last_pos_change_instant = Some(now);

                }

            } else {

                self.smoothed_time = est.max(self.smoothed_time);

            }

            self.smoothed_time

        } else {

            0.0

        }

    }

}



// ============================================================================

// CONFIGURACIÓN, CLONES DE NOTAS Y EFECTOS VISUALES

// ============================================================================

#[derive(Clone, Debug)]

struct GameSettings {

    pgood_mode: bool,

    e_and_l_tilt: bool,

    auto_mode: bool,

    hitsound: bool,

    separate_misses: bool,

    user_offset: f64,

}



impl Default for GameSettings {

    fn default() -> Self {

        Self {

            pgood_mode: true,

            e_and_l_tilt: true,

            auto_mode: false,

            hitsound: true,

            separate_misses: false,

            user_offset: 0.0,

        }

    }

}



fn parse_bool_setting(value: &str, default: bool) -> bool {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => true,
        "0" | "false" | "off" | "no" => false,
        _ => default,
    }
}

impl GameSettings {
    fn settings_path() -> PathBuf {
        #[cfg(target_os = "android")]
        {
            PathBuf::from(ANDROID_APP_FILES_DIR).join("settings.txt")
        }

        #[cfg(not(target_os = "android"))]
        {
            let base = if cfg!(target_os = "windows") {
                std::env::var("APPDATA").ok().map(PathBuf::from)
            } else {
                std::env::var("XDG_CONFIG_HOME")
                    .ok()
                    .map(PathBuf::from)
                    .or_else(|| {
                        std::env::var("HOME")
                            .ok()
                            .map(|h| PathBuf::from(h).join(".config"))
                    })
            };
            base.unwrap_or_else(|| PathBuf::from("."))
                .join("taiko_tja_rust")
                .join("settings.txt")
        }
    }

    fn fallback_path() -> PathBuf {
        std::env::temp_dir().join("taiko_tja_rust_settings.txt")
    }

    fn load() -> Self {
        let mut s = Self::default();
        let text = fs::read_to_string(Self::settings_path())
            .or_else(|_| fs::read_to_string(Self::fallback_path()));
        let Ok(text) = text else { return s };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "pgood_mode" => s.pgood_mode = parse_bool_setting(value, s.pgood_mode),
                "e_and_l_tilt" => s.e_and_l_tilt = parse_bool_setting(value, s.e_and_l_tilt),
                "auto_mode" => s.auto_mode = parse_bool_setting(value, s.auto_mode),
                "hitsound" => s.hitsound = parse_bool_setting(value, s.hitsound),
                "separate_misses" => {
                    s.separate_misses = parse_bool_setting(value, s.separate_misses)
                }
                "user_offset" => s.user_offset = value.trim().parse().unwrap_or(0.0),
                _ => {}
            }
        }
        s
    }

    fn save(&self) {
        let text = format!(
            "pgood_mode={}\ne_and_l_tilt={}\nauto_mode={}\nhitsound={}\nseparate_misses={}\nuser_offset={}\n",
            self.pgood_mode,
            self.e_and_l_tilt,
            self.auto_mode,
            self.hitsound,
            self.separate_misses,
            self.user_offset
        );
        let path = Self::settings_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::write(&path, &text).is_err() {
            let _ = fs::write(Self::fallback_path(), text);
        }
    }
}



#[derive(Clone, Copy, Debug, PartialEq, Eq)]

enum NoteKind {

    Don,

    Ka,

    BigDon,

    BigKa,

    RollS,

    BigRollS,

    Balloon,
    Barline,

}



impl NoteKind {

    fn from_char(c: char) -> Option<Self> {

        match c {

            '1' => Some(NoteKind::Don),

            '2' => Some(NoteKind::Ka),

            '3' => Some(NoteKind::BigDon),

            '4' => Some(NoteKind::BigKa),

            '5' => Some(NoteKind::RollS),

            '6' => Some(NoteKind::BigRollS),

            '7' | '9' => Some(NoteKind::Balloon),

            _ => None,

        }

    }



    fn is_normal(&self) -> bool {

        matches!(

            self,

            NoteKind::Don | NoteKind::Ka | NoteKind::BigDon | NoteKind::BigKa

        )

    }



    fn is_don(&self) -> bool {

        matches!(self, NoteKind::Don | NoteKind::BigDon)

    }



    fn is_roll(&self) -> bool {

        matches!(self, NoteKind::RollS | NoteKind::BigRollS)

    }



    fn is_balloon(&self) -> bool {

        matches!(self, NoteKind::Balloon)

    }



    fn is_big(&self) -> bool {

        matches!(

            self,

            NoteKind::BigDon | NoteKind::BigKa | NoteKind::BigRollS

        )

    }

    fn is_barline(&self) -> bool { matches!(self, NoteKind::Barline) }

}



#[derive(Clone, Copy, Debug, PartialEq, Eq)]

enum NoteState {

    Active,

    Hit,

    Miss,

    Finished,

}



#[derive(Clone, Debug)]

struct NoteClone {

    kind: NoteKind,

    hit_time: f64,

    velocity: f64,

    state: NoteState,

    roll_id: Option<usize>,

    hits_needed: i32,

    hits: i32,

    x_pos: f64,

}



#[derive(Clone, Copy, Debug, PartialEq, Eq)]

enum JudgeKind {

    PGood,

    Good,

    Ok,

    Bad,

}



struct JudgePopup {

    kind: JudgeKind,

    tilt_deg: f32,

    age_frames: f32,

}



struct JudgeBurst {

    kind: JudgeKind,

    age_frames: f32,

}



struct FlyingNote {

    kind: NoteKind,

    anim: f32,

    frame_count: i32,

    exploded: bool,

    fade_frames: f32,

}



struct BigGlowTrail {

    x: f32,

    y: f32,

    age_frames: f32,

}



struct ExplosionFx {

    x: f32,

    y: f32,

    size: f32,

    angle: f32,

    color_fx: f32,

    age_frames: f32,

}



struct FireworkFx {

    x: f32,

    y: f32,

    hue: f32,

    delay_frames: f32,

    age_frames: f32,

    big: bool,

}



struct DancerFx {

    is_miss: bool,

    x: f32,

    y: f32,

    size: f32,

    color_fx: f32,

    bpm: f64,

    age_frames: f32,

}



struct ConfettiFx {

    x: f32,

    y: f32,

    color_fx: f32,

    age_frames: f32,

}



struct ScoreAddPopup {

    text: String,

    age_frames: f32,

}



struct RollFlashFx {

    age_frames: f32,

}



struct BalloonPopFx {

    age_frames: f32,

}



struct DrumFlash {
    don_l: f32,
    don_r: f32,
    ka_l: f32,
    ka_r: f32,
    don_l_age: f32,
    don_r_age: f32,
    ka_l_age: f32,
    ka_r_age: f32,
    drum_bounce: f32,
    lane_flash_kind: Option<bool>,
    lane_flash_alpha: f32,
    lane_flash_age: f32,
    judge_ring_kind: JudgeKind,
    judge_ring_alpha: f32,
    judge_ring_age: f32,
    judge_lane_alpha: f32,
    judge_lane_age: f32,
    auto_left: bool,
}



impl Default for DrumFlash {

    fn default() -> Self {

        Self {

            don_l: 0.0,

            don_r: 0.0,

            ka_l: 0.0,

            ka_r: 0.0,

            don_l_age: 999.0,
            don_r_age: 999.0,
            ka_l_age: 999.0,
            ka_r_age: 999.0,

            drum_bounce: 0.0,

            lane_flash_kind: None,
            lane_flash_alpha: 0.0,
            lane_flash_age: 999.0,
            judge_ring_kind: JudgeKind::PGood,
            judge_ring_alpha: 0.0,
            judge_ring_age: 999.0,
            judge_lane_alpha: 0.0,
            judge_lane_age: 999.0,
            auto_left: true,

        }

    }

}



fn flash_auto_side(flash: &mut DrumFlash, is_don: bool) {
    let left = flash.auto_left;
    flash.auto_left = !flash.auto_left;
    flash.drum_bounce = 1.0;
    flash.lane_flash_kind = Some(is_don);
    flash.lane_flash_alpha = 1.0;
    flash.lane_flash_age = 0.0;
    if is_don {
        if left {
            flash.don_l = 1.0;
            flash.don_l_age = 0.0;
        } else {
            flash.don_r = 1.0;
            flash.don_r_age = 0.0;
        }
    } else if left {
        flash.ka_l = 1.0;
        flash.ka_l_age = 0.0;
    } else {
        flash.ka_r = 1.0;
        flash.ka_r_age = 0.0;
    }
}

fn cap_vec_front<T>(v: &mut Vec<T>, max_len: usize) {

    if v.len() > max_len {

        let excess = v.len() - max_len;

        v.drain(0..excess);

    }

}



// ============================================================================

// ESTADO DE LA PARTIDA

// ============================================================================

struct PlaySession {

    course: Course,

    total_notes: u32,

    bpm_actual: f64,

    bpm_display: f64,

    firma_tiempo: f64,

    duracion_compas: f64,

    compas_actual: usize,

    velocidad_scroll: f64,

    tiempo_actual: f64,

    tamashii_add: f64,

    tamashii: f64,



    gogo_events: Vec<(f64, bool)>,

    gogo_idx: usize,

    gogo_active: bool,

    gogo_anim: f32,

    soul_flash_timer: f32,

    soul_rainbow_color_fx: f32,
    soul_rainbow_fade: f32,

    fire_frame_idx: usize,

    fire_frame_timer: f32,



    lyric_events: Vec<(f64, String)>,

    lyric_idx: usize,

    lyric_actual: String,



    bpm_events: Vec<(f64, f64)>,

    bpm_idx: usize,



    redoble_inicio: Vec<f64>,

    redoble_fin: Vec<Option<f64>>,

    redoble_abierto: bool,



    balloons_list: Vec<i32>,

    balloon_idx: usize,

    globo_restante: i32,

    globo_pop_id: Option<usize>,



    roll_contador: i32,

    roll_ultimo_golpe: f64,

    roll_pop_anim: f32,
    roll_pop_anim_age: f32,

    balloon_pop_anim: f32,
    balloon_pop_anim_age: f32,



    notes: Vec<NoteClone>,



    score: i64,

    score_anim: f32,
    score_anim_age: f32,

    combo: u32,

    combo_anim: f32,
    combo_anim_age: f32,

    max_combo: u32,

    pgood: u32,

    good: u32,

    ok: u32,

    bad: u32,

    miss: u32,



    judge_popups: Vec<JudgePopup>,

    judge_bursts: Vec<JudgeBurst>,

    flying_notes: Vec<FlyingNote>,

    big_glows: Vec<BigGlowTrail>,

    explosions: Vec<ExplosionFx>,

    fireworks: Vec<FireworkFx>,

    dancers: Vec<DancerFx>,

    confetti: Vec<ConfettiFx>,

    score_popups: Vec<ScoreAddPopup>,

    roll_flashes: Vec<RollFlashFx>,

    balloon_pops: Vec<BalloonPopFx>,

    drum_flash: DrumFlash,

    auto_roll_cd: f32,
    auto_balloon_cd: f32,



    finished: bool,

    finish_timer: f32,

    finish_anim_frames: f32,

    fallback_clock: f64,

    rainbow_scroll: f32,
    barline_off: bool,

}



impl PlaySession {

    fn new(tja: &TjaData, course: Course, settings: &GameSettings) -> Self {

        let course_data = tja.courses.get(&course).cloned().unwrap_or_default();

        let total_notes = course_data.note_count.max(1);

        let bpm = if tja.bpm > 0.0 { tja.bpm } else { 120.0 };

        let duracion_compas = (240.0 / bpm) * 1.0;

        let tiempo_actual = settings.user_offset - tja.offset;

        let tamashii_add = 10000.0 / (0.715 * (total_notes as f64));



        Self {

            course,

            total_notes,

            bpm_actual: bpm,

            bpm_display: bpm,

            firma_tiempo: 1.0,

            duracion_compas,

            compas_actual: 0,

            velocidad_scroll: 300.0,

            tiempo_actual,

            tamashii_add,

            tamashii: 0.0,

            gogo_events: Vec::new(),

            gogo_idx: 0,

            gogo_active: false,

            gogo_anim: 0.0,

            soul_flash_timer: 0.0,

            soul_rainbow_color_fx: 0.0,
            soul_rainbow_fade: 0.0,

            fire_frame_idx: 0,

            fire_frame_timer: 0.0,

            lyric_events: Vec::new(),

            lyric_idx: 0,

            lyric_actual: String::new(),

            bpm_events: Vec::new(),

            bpm_idx: 0,

            redoble_inicio: Vec::new(),

            redoble_fin: Vec::new(),

            redoble_abierto: false,

            balloons_list: course_data.balloons,

            balloon_idx: 0,

            globo_restante: 0,

            globo_pop_id: None,

            roll_contador: 0,

            roll_ultimo_golpe: -10.0,

            roll_pop_anim: 0.0,
            roll_pop_anim_age: 999.0,

            balloon_pop_anim: 0.0,
            balloon_pop_anim_age: 999.0,

            notes: Vec::new(),

            score: 0,

            score_anim: 0.0,
            score_anim_age: 999.0,

            combo: 0,

            combo_anim: 0.0,
            combo_anim_age: 999.0,

            max_combo: 0,

            pgood: 0,

            good: 0,

            ok: 0,

            bad: 0,

            miss: 0,

            judge_popups: Vec::new(),

            judge_bursts: Vec::new(),

            flying_notes: Vec::new(),

            big_glows: Vec::new(),

            explosions: Vec::new(),

            fireworks: Vec::new(),

            dancers: Vec::new(),

            confetti: Vec::new(),

            score_popups: Vec::new(),

            roll_flashes: Vec::new(),

            balloon_pops: Vec::new(),

            drum_flash: DrumFlash::default(),

            auto_roll_cd: 0.0,
            auto_balloon_cd: 0.0,

            finished: false,

            finish_timer: 0.0,

            finish_anim_frames: 0.0,

            fallback_clock: 0.0,

            rainbow_scroll: 0.0,
            barline_off: false,

        }

    }



    fn measure_min_velocity(&self, measure_text: &str) -> f64 {
        let mut scroll = self.velocidad_scroll;
        let mut bpm = self.bpm_actual;
        let mut min_vel = (scroll / 160.0) * bpm;
        if !min_vel.is_finite() || min_vel <= 0.0 { min_vel = 1.0; }
        for raw in measure_text.lines() {
            let l = strip_comment(raw).trim();
            if !l.starts_with('#') { continue; }
            let upper = l.to_uppercase();
            if upper.starts_with("#BPMCHANGE") { if let Ok(v)=l[10..].trim().parse::<f64>() { if v>0.0 { bpm=v; } } }
            else if upper.starts_with("#SCROLL") { if let Ok(v)=l[7..].trim().parse::<f64>() { scroll=v*300.0; } }
            let vel = (scroll/160.0)*bpm;
            if vel.is_finite() && vel>0.0 { min_vel=min_vel.min(vel); }
        }
        min_vel.max(1.0)
    }

    fn generar_compas(&mut self, measure_text: &str) {

        let lines: Vec<String> = measure_text

            .lines()

            .map(|l| strip_comment(l).trim().to_string())

            .collect();



        let mut notas_compas = String::new();

        for l in &lines {

            if !l.starts_with('#') {

                for ch in l.chars() {

                    if ch.is_ascii_digit() {

                        notas_compas.push(ch);

                    }

                }

            }

        }

        let total_notas = notas_compas.chars().count();



        for l in &lines {

            if l.starts_with('#') {

                let upper = l.to_uppercase();

                if upper.starts_with("#BPMCHANGE") {

                    if let Ok(new_bpm) = l[10..].trim().parse::<f64>() {

                        if new_bpm > 0.0 {

                            self.bpm_actual = new_bpm;

                            self.duracion_compas = (240.0 / self.bpm_actual) * self.firma_tiempo;

                            self.bpm_events.push((self.tiempo_actual, new_bpm));

                        }

                    }

                } else if upper.starts_with("#MEASURE") {

                    let rest = l[8..].trim();

                    let parts: Vec<&str> = rest.split('/').collect();

                    if parts.len() == 2 {

                        if let (Ok(num), Ok(den)) = (

                            parts[0].trim().parse::<f64>(),

                            parts[1].trim().parse::<f64>(),

                        ) {

                            if den != 0.0 {

                                self.firma_tiempo = num / den;

                                self.duracion_compas =

                                    (240.0 / self.bpm_actual) * self.firma_tiempo;

                            }

                        }

                    }

                } else if upper.starts_with("#SCROLL") {

                    if let Ok(scroll) = l[7..].trim().parse::<f64>() {

                        self.velocidad_scroll = scroll * 300.0;

                    }

                } else if upper.starts_with("#GOGOSTART") {

                    self.gogo_events.push((self.tiempo_actual, true));

                } else if upper.starts_with("#GOGOEND") {

                    self.gogo_events.push((self.tiempo_actual, false));

                } else if upper.starts_with("#LYRIC") {

                    let lyric_text = if l.len() > 6 { l[6..].trim() } else { "" };

                    self.lyric_events

                        .push((self.tiempo_actual, lyric_text.to_string()));

                }

            } else {

                for ch in l.chars() {

                    if !ch.is_ascii_digit() {

                        continue;

                    }



                    if self.redoble_abierto && ch != '0' {

                        let t_fin = (self.tiempo_actual * 1_000_000_000.0).round() / 1_000_000_000.0;

                        if let Some(last) = self.redoble_fin.last_mut() {

                            *last = Some(t_fin);

                        }

                        self.redoble_abierto = false;

                    }



                    let mut cur_roll_id = None;

                    let mut cur_hits_needed = 0;



                    if matches!(ch, '5' | '6' | '7' | '9') {

                        let t_ini = (self.tiempo_actual * 1_000_000_000.0).round() / 1_000_000_000.0;

                        self.redoble_inicio.push(t_ini);

                        self.redoble_fin.push(None);

                        let rid = self.redoble_inicio.len() - 1;

                        cur_roll_id = Some(rid);

                        self.redoble_abierto = true;



                        if ch == '7' || ch == '9' {

                            cur_hits_needed = self

                                .balloons_list

                                .get(self.balloon_idx)

                                .copied()

                                .unwrap_or(5);

                            self.balloon_idx += 1;

                        }

                    }



                    if ch != '0' && ch != '8' {

                        if let Some(kind) = NoteKind::from_char(ch) {

                            let vel = (self.velocidad_scroll / 160.0) * self.bpm_actual;

                            self.notes.push(NoteClone {

                                kind,

                                hit_time: self.tiempo_actual,

                                velocity: vel,

                                state: NoteState::Active,

                                roll_id: cur_roll_id,

                                hits_needed: cur_hits_needed,

                                hits: 0,

                                x_pos: X_JUEZ,

                            });

                        }

                    }



                    if total_notas > 0 {

                        self.tiempo_actual += self.duracion_compas / (total_notas as f64);

                    }

                }

            }

        }



        if total_notas == 0 {

            self.tiempo_actual += self.duracion_compas;

        }

    }



    fn note_score_pgood(&self) -> i64 {

        let base = ((1_000_000.0 / (self.total_notes as f64)) / 10.0).round() as i64 * 10;

        base + 10

    }



    fn note_score_good(&self) -> i64 {

        ((1_000_000.0 / (self.total_notes as f64)) / 10.0).round() as i64 * 10

    }



    fn note_score_ok(&self) -> i64 {

        ((1_000_000.0 / (self.total_notes as f64)) / 20.0).round() as i64 * 10

    }

    fn clear_threshold(&self) -> f64 {
        match self.course {
            Course::Kantan => 6000.0,
            Course::Futsuu | Course::Muzukashii => 7000.0,
            Course::Oni | Course::UraOni => 8000.0,
        }
    }

    fn is_clear(&self) -> bool { self.tamashii >= self.clear_threshold() }



    fn add_score(&mut self, delta: i64) {

        self.score += delta;

        self.score_anim = 1.0;
        self.score_anim_age = 0.0;

        if delta > 0 {

            self.score_popups.push(ScoreAddPopup {

                text: delta.to_string(),

                age_frames: 0.0,

            });

            cap_vec_front(&mut self.score_popups, 32);

        }

    }



    fn spawn_flying_note(&mut self, kind: NoteKind) {

        if self.gogo_active && self.tamashii >= 8000.0 {

            let big = kind.is_big();

            for _ in 0..8 {

                self.fireworks.push(FireworkFx {

                    x: rand::gen_range(-135.0, 135.0),

                    y: rand::gen_range(130.0, 160.0),

                    hue: rand::gen_range(0.0, 360.0),

                    delay_frames: rand::gen_range(0.0, 45.0),

                    age_frames: 0.0,

                    big,

                });

            }

            cap_vec_front(&mut self.fireworks, 240);

        }

        self.flying_notes.push(FlyingNote {

            kind,

            anim: 0.0,

            frame_count: 0,

            exploded: false,

            fade_frames: 0.0,

        });

        cap_vec_front(&mut self.flying_notes, 140);

    }



    fn spawn_dancer(&mut self, is_miss: bool) {

        let color_fx = rand::gen_range(1, 5) as f32 * 50.0;

        self.dancers.push(DancerFx {

            is_miss,

            x: -230.0,

            y: 120.0,

            size: 0.0,

            color_fx,

            bpm: self.bpm_display,

            age_frames: 0.0,

        });

        cap_vec_front(&mut self.dancers, 180);

    }



    fn register_judge(

        &mut self,

        judge: JudgeKind,

        tilt_val: f64,

        note_kind: NoteKind,

        settings: &GameSettings,

    ) {

        let judge = if judge == JudgeKind::PGood && !settings.pgood_mode {

            JudgeKind::Good

        } else {

            judge

        };



        let tilt_deg = (tilt_val * 500.0) as f32;

        self.judge_popups.push(JudgePopup {

            kind: judge,

            tilt_deg,

            age_frames: 0.0,

        });

        cap_vec_front(&mut self.judge_popups, 32);



        if judge != JudgeKind::Bad {

            self.judge_bursts.push(JudgeBurst {

                kind: judge,

                age_frames: 0.0,

            });

            cap_vec_front(&mut self.judge_bursts, 32);

            self.drum_flash.judge_ring_kind = judge;

            self.drum_flash.judge_ring_alpha = 1.0;
            self.drum_flash.judge_ring_age = 0.0;
            self.drum_flash.judge_lane_alpha = 1.0;
            self.drum_flash.judge_lane_age = 0.0;

            self.spawn_dancer(false);

        } else {

            self.spawn_dancer(true);

        }



        match judge {

            JudgeKind::PGood => {

                self.pgood += 1;

                self.combo += 1;

                self.max_combo = self.max_combo.max(self.combo);

                self.combo_anim = 1.0;
                self.combo_anim_age = 0.0;

                self.tamashii = (self.tamashii + self.tamashii_add).clamp(0.0, 10000.0);

                let pts = self.note_score_pgood();

                self.add_score(pts);

                self.spawn_flying_note(note_kind);

            }

            JudgeKind::Good => {

                self.good += 1;

                self.combo += 1;

                self.max_combo = self.max_combo.max(self.combo);

                self.combo_anim = 1.0;
                self.combo_anim_age = 0.0;

                self.tamashii = (self.tamashii + self.tamashii_add).clamp(0.0, 10000.0);

                let pts = self.note_score_good();

                self.add_score(pts);

                self.spawn_flying_note(note_kind);

            }

            JudgeKind::Ok => {

                self.ok += 1;

                self.combo += 1;

                self.max_combo = self.max_combo.max(self.combo);

                self.combo_anim = 1.0;
                self.combo_anim_age = 0.0;

                self.tamashii = (self.tamashii + self.tamashii_add * 0.5).clamp(0.0, 10000.0);

                let pts = self.note_score_ok();

                self.add_score(pts);

                self.spawn_flying_note(note_kind);

            }

            JudgeKind::Bad => {

                self.bad += 1;

                self.combo = 0;

                self.tamashii = (self.tamashii - self.tamashii_add * 2.0).clamp(0.0, 10000.0);

            }

        }

    }



    fn accuracy_percent(&self) -> f64 {

        let hit_val = (self.pgood as f64) + (self.good as f64) + (self.ok as f64) * 0.5;

        (hit_val / (self.total_notes as f64)) * 100.0

    }



    fn rank_label(&self) -> &'static str {

        let p = self.accuracy_percent();

        if p >= 100.0 && self.pgood >= self.total_notes {

            "P"

        } else if p >= 97.5 {

            "S+"

        } else if p >= 95.0 {

            "S"

        } else if p >= 90.0 {

            "A+"

        } else if p >= 80.0 {

            "A"

        } else if p >= 70.0 {

            "B"

        } else if p >= 55.0 {

            "C"

        } else if p >= 40.0 {

            "D"

        } else if p >= 20.0 {

            "E"

        } else {

            "F"

        }

    }

}



// ============================================================================

// UTILIDADES DE DIBUJO Y COORDENADAS (Scratch 640x360 <-> Pantalla)

// ============================================================================

enum AppState {

    NeedFile,

    FileBrowser,

    SelectDifficulty,

    SettingsMenu,

    Playing,

    Finished,

}



fn scratch_to_screen(sx: f32, sy: f32) -> (f32, f32, f32) {

    let w = screen_width();

    let h = screen_height();

    let scale = (w / STAGE_W).min(h / STAGE_H);

    let cx = w * 0.5;

    let cy = h * 0.5;

    (cx + sx * scale, cy - sy * scale, scale)

}



fn screen_to_scratch(px: f32, py: f32) -> (f32, f32) {

    let w = screen_width();

    let h = screen_height();

    let scale = (w / STAGE_W).min(h / STAGE_H).max(0.0001);

    let sx = (px - w * 0.5) / scale;

    let sy = (h * 0.5 - py) / scale;

    (sx, sy)

}



fn is_inside_drum(drum_sp: Option<&SpriteTex>, sx: f32, sy: f32) -> bool {

    let (w, h, cx, cy) = match drum_sp {

        Some(sp) => (sp.w, sp.h, sp.cx, sp.cy),

        None => (381.0, 145.5, 190.5, -34.5),

    };

    let half_w = (w * 0.5).max(1.0);

    let center_x = -cx + half_w;

    let bottom_y = cy - h;

    if sy > cy || sy < bottom_y - 40.0 || sx < -cx || sx > -cx + w {

        return false;

    }

    let dx = (sx - center_x) / half_w;

    let dy = ((sy - bottom_y).max(0.0)) / h.max(1.0);

    dx * dx + dy * dy <= 1.05

}



fn hsv_to_color(h_deg: f32, s: f32, v: f32, a: f32) -> Color {

    let h = ((h_deg % 360.0) + 360.0) % 360.0 / 60.0;

    let i = h.floor() as i32;

    let f = h - (i as f32);

    let p = v * (1.0 - s);

    let q = v * (1.0 - s * f);

    let t = v * (1.0 - s * (1.0 - f));

    let (r, g, b) = match i {

        0 => (v, t, p),

        1 => (q, v, p),

        2 => (p, v, t),

        3 => (p, q, v),

        4 => (t, p, v),

        _ => (v, p, q),

    };

    Color::new(r, g, b, a)

}



fn draw_sprite_fx(

    mats: &RenderMaterials,

    additive: bool,

    hue_shift_deg: f32,

    brightness: f32,

    sp: Option<&SpriteTex>,

    sx: f32,

    sy: f32,

    scale_x: f32,

    scale_y: f32,

    rot_deg: f32,

    tint: Color,

) {

    let Some(sp) = sp else { return };

    let mat = if additive {

        &mats.additive_fx

    } else {

        &mats.normal_fx

    };

    let use_shader = additive || hue_shift_deg != 0.0 || brightness > 0.0;

    if use_shader {

        mat.set_uniform("u_hue_shift", hue_shift_deg);

        mat.set_uniform("u_brightness", brightness);

        gl_use_material(mat);

    }

    draw_sprite_ex(Some(sp), sx, sy, scale_x, scale_y, rot_deg, tint);

    if use_shader {

        gl_use_default_material();

    }

}



fn draw_sprite_ex(

    sp: Option<&SpriteTex>,

    sx: f32,

    sy: f32,

    scale_x: f32,

    scale_y: f32,

    rot_deg: f32,

    tint: Color,

) {

    let Some(sp) = sp else { return };

    let (anchor_x, anchor_y, stage_scale) = scratch_to_screen(sx, sy);

    let dw = sp.w * scale_x * stage_scale;

    let dh = sp.h * scale_y * stage_scale;

    let x = anchor_x - sp.cx * scale_x * stage_scale;

    let y = anchor_y - sp.cy * scale_y * stage_scale;

    draw_texture_ex(

        &sp.tex,

        x,

        y,

        tint,

        DrawTextureParams {

            dest_size: Some(vec2(dw, dh)),

            rotation: rot_deg.to_radians(),

            pivot: Some(vec2(anchor_x, anchor_y)),

            ..Default::default()

        },

    );

}



fn draw_sprite(sp: Option<&SpriteTex>, sx: f32, sy: f32, scale: f32, tint: Color) {

    draw_sprite_ex(sp, sx, sy, scale, scale, 0.0, tint);

}



fn draw_sprite_clipped_x(

    sp: Option<&SpriteTex>,

    sx: f32,

    sy: f32,

    scale_x: f32,

    scale_y: f32,

    clip_x1: f32,

    clip_x2: f32,

    tint: Color,

) {

    let Some(sp) = sp else { return };

    let left_s = sx - sp.cx * scale_x;

    let right_s = left_s + sp.w * scale_x;

    let c1 = left_s.max(clip_x1);

    let c2 = right_s.min(clip_x2);

    if c2 <= c1 {

        return;

    }



    let u1 = (c1 - left_s) / (sp.w * scale_x);

    let u2 = (c2 - left_s) / (sp.w * scale_x);

    let top_s = sy + sp.cy * scale_y;

    let (screen_x, screen_y, stage_scale) = scratch_to_screen(c1, top_s);

    let dw = (c2 - c1) * stage_scale;

    let dh = sp.h * scale_y * stage_scale;

    let tw = sp.tex.width();

    let th = sp.tex.height();



    draw_texture_ex(

        &sp.tex,

        screen_x,

        screen_y,

        tint,

        DrawTextureParams {

            dest_size: Some(vec2(dw, dh)),

            source: Some(Rect::new(u1 * tw, 0.0, (u2 - u1) * tw, th)),

            ..Default::default()

        },

    );

}



fn draw_drumroll_body(x_ini: f32, x_fin: f32, cy: f32, is_big: bool) {
    let w = (x_fin - x_ini).max(0.0);
    if w <= 0.0 { return; }
    // Heights match head/tail SVGs: small 33px (9e0fdb.../730eac...), big 43px (70a878.../d868b7...)
    let (h_black, h_white, h_fill) = if is_big { (43.0, 39.0, 35.0) } else { (33.0, 29.0, 25.0) };
    draw_rect_scratch(x_ini, cy + h_black * 0.5, w, h_black, BLACK);
    draw_rect_scratch(x_ini, cy + h_white * 0.5, w, h_white, WHITE);
    draw_rect_scratch(x_ini, cy + h_fill * 0.5, w, h_fill, Color::from_rgba(255, 230, 0, 255)); // #FFE600
}



fn draw_digits_row(

    digits_tex: &[Option<SpriteTex>],

    text: &str,

    center_x: f32,

    sy: f32,

    size_percent: f32,

    stretch_y: f32,

    tint: Color,

) {

    let chars: Vec<char> = text.chars().filter(|c| c.is_ascii_digit()).collect();

    let len = chars.len() as f32;

    let step = size_percent / 8.0;

    let scale = size_percent / 100.0;

    for (i, ch) in chars.iter().enumerate() {

        let id = (i as f32) + 1.0;

        let x = ((id - 0.5) - len * 0.5) * step + center_x;

        let d_idx = (*ch as u8 - b'0') as usize;

        if let Some(sp) = digits_tex.get(d_idx).and_then(|o| o.as_ref()) {
            let y = sy + sp.h * scale * (stretch_y - 1.0) * 0.25;
            draw_sprite_ex(Some(sp), x, y, scale, scale * stretch_y, 0.0, tint);
        }

    }

}



fn draw_digits_right_aligned(

    digits_tex: &[Option<SpriteTex>],

    text: &str,

    anchor_x: f32,

    sy: f32,

    size_percent: f32,

    stretch_y: f32,

    tint: Color,

) {

    let chars: Vec<char> = text.chars().filter(|c| c.is_ascii_digit()).collect();

    let len = chars.len() as f32;

    let step = size_percent / 8.0;

    let scale = size_percent / 100.0;

    for (i, ch) in chars.iter().enumerate() {

        let id = (i as f32) + 1.0;

        let x = ((id - 0.5) - len) * step + anchor_x;

        let d_idx = (*ch as u8 - b'0') as usize;

        if let Some(sp) = digits_tex.get(d_idx).and_then(|o| o.as_ref()) {
            let y = sy + sp.h * scale * (stretch_y - 1.0) * 0.25;
            draw_sprite_ex(Some(sp), x, y, scale, scale * stretch_y, 0.0, tint);
        }

    }

}



fn draw_score_add_digits(digits_tex: &[Option<SpriteTex>], text: &str, age_frames: f32) {

    let chars: Vec<char> = text.chars().filter(|c| c.is_ascii_digit()).collect();

    let len = chars.len() as f32;

    let size_percent = 75.0_f32;

    let step = size_percent / 8.0;

    let scale = size_percent / 100.0;



    for (i, ch) in chars.iter().enumerate() {

        let id = (i as f32) + 1.0;

        let f = age_frames;

        let max_f = 26.0 + id;

        if f >= max_f {

            continue;

        }

        let base_x = ((id - 0.5) - len) * step - 186.0;

        let x_off = if f < 1.0 {

            0.0

        } else if f < 4.0 {

            -(f - 1.0) * 4.0

        } else if f < 8.0 {

            -12.0 + (f - 4.0) * 0.75

        } else {

            -9.0

        };

        let t_wait = 18.0 + id;

        let y_off = if f < t_wait {

            0.0

        } else if f < t_wait + 3.0 {

            (f - t_wait) * 2.0

        } else {

            6.0 - (f - (t_wait + 3.0)) * 2.0

        };

        let alpha = if f < 1.0 {

            0.25

        } else if f < 4.0 {

            0.25 + (f - 1.0) * 0.25

        } else if f < t_wait + 3.0 {

            1.0

        } else {

            (1.0 - (f - (t_wait + 3.0)) * 0.125).max(0.0)

        };

        let d_idx = (*ch as u8 - b'0') as usize;

        if let Some(sp) = digits_tex.get(d_idx).and_then(|o| o.as_ref()) {

            draw_sprite_ex(

                Some(sp),

                base_x + x_off,

                85.0 + y_off,

                scale,

                scale,

                0.0,

                Color::from_rgba(255, 84, 0, (alpha * 255.0) as u8),

            );

        }

    }

}



fn draw_text_aligned(

    text: &str,

    font: Option<&Font>,

    sx: f32,

    sy: f32,

    size_s: f32,

    color: Color,

    align: f32,

) {

    let (x, y, scale) = scratch_to_screen(sx, sy);

    let font_size = (size_s * scale).max(8.0) as u16;

    let dims = measure_text(text, font, font_size, 1.0);

    draw_text_ex(

        text,

        x - dims.width * align,

        y + dims.height * 0.35,

        TextParams {

            font,

            font_size,

            color,

            ..Default::default()

        },

    );

}



fn draw_text_outlined(

    text: &str,

    font: Option<&Font>,

    sx: f32,

    sy: f32,

    size_s: f32,

    color: Color,

    outline: Color,

    align: f32,

) {

    let (_, _, scale) = scratch_to_screen(0.0, 0.0);

    let off = 1.6;

    for &(dx, dy) in &[

        (-off, 0.0),

        (off, 0.0),

        (0.0, -off),

        (0.0, off),

        (-off, -off),

        (off, -off),

        (-off, off),

        (off, off),

    ] {

        let (x, y, _) = scratch_to_screen(sx + dx, sy + dy);

        let font_size = (size_s * scale).max(8.0) as u16;

        let dims = measure_text(text, font, font_size, 1.0);

        draw_text_ex(

            text,

            x - dims.width * align,

            y + dims.height * 0.35,

            TextParams {

                font,

                font_size,

                color: outline,

                ..Default::default()

            },

        );

    }

    draw_text_aligned(text, font, sx, sy, size_s, color, align);

}



fn draw_rect_scratch(sx: f32, sy: f32, sw: f32, sh: f32, color: Color) {

    let (x, y, scale) = scratch_to_screen(sx, sy);

    draw_rectangle(x, y, sw * scale, sh * scale, color);

}



fn point_in_scratch_rect(mx: f32, my: f32, sx: f32, sy: f32, sw: f32, sh: f32) -> bool {

    let (x, y, scale) = scratch_to_screen(sx, sy);

    mx >= x && mx <= x + sw * scale && my >= y && my <= y + sh * scale

}



#[cfg(not(target_os = "android"))]

fn pick_audio_manually(audio: &mut AudioEngine, default_dir: Option<&PathBuf>) {

    let mut dialog = rfd::FileDialog::new()

        .add_filter(

            "Audio (.ogg, .mp3, .wav, .flac, .m4a)",

            &["ogg", "mp3", "wav", "flac", "m4a"],

        )

        .set_title("Selecciona el archivo de audio de la canción");

    if let Some(dir) = default_dir {

        dialog = dialog.set_directory(dir);

    }

    if let Some(apath) = dialog.pick_file() {

        if let Err(e) = audio.load_song_from_path(&apath) {

            audio.last_audio_error = Some(e);

        }

    }

}



#[cfg(not(target_os = "android"))]

fn open_tja_and_audio_desktop(audio: &mut AudioEngine) -> Option<TjaData> {

    let tja_path = rfd::FileDialog::new()

        .add_filter("TJA Chart", &["tja"])

        .set_title("Selecciona un archivo .tja")

        .pick_file()?;



    let bytes = fs::read(&tja_path).ok()?;

    let text = decode_tja_bytes(&bytes);

    let tja_dir = tja_path.parent().map(|p| p.to_path_buf());

    let tja = parse_tja(&text, tja_dir.clone());



    audio.clear_song();



    if let Some(apath) = find_tja_audio(&tja) {

        if let Err(e) = audio.load_song_from_path(&apath) {

            audio.last_audio_error = Some(e);

        }

    } else {

        pick_audio_manually(audio, tja_dir.as_ref());

    }

    Some(tja)

}



fn start_demo_preview(audio: &mut AudioEngine, tja: &TjaData) {

    if audio.song_bytes.is_some() {

        if let Err(e) = audio.start_song(tja.demo_start, tja.song_vol) {

            audio.last_audio_error = Some(e);

        }

    }

}



struct InputState {

    don_hit: bool,

    don_left: bool,

    don_right: bool,

    ka_hit: bool,

    ka_left: bool,

    ka_right: bool,

}



fn poll_drum_input(drum_sp: Option<&SpriteTex>) -> InputState {

    let mut st = InputState {

        don_left: is_key_pressed(KeyCode::F)

            || is_key_pressed(KeyCode::V)

            || is_key_pressed(KeyCode::R),

        don_right: is_key_pressed(KeyCode::J)

            || is_key_pressed(KeyCode::N)

            || is_key_pressed(KeyCode::U),

        ka_left: is_key_pressed(KeyCode::D)

            || is_key_pressed(KeyCode::C)

            || is_key_pressed(KeyCode::E),

        ka_right: is_key_pressed(KeyCode::K)

            || is_key_pressed(KeyCode::M)

            || is_key_pressed(KeyCode::I),

        don_hit: false,

        ka_hit: false,

    };



    // Multi-touch (Sprite8 del project.json):

    // Tocar dentro de Sprite7 -> Don (x < 0 izq, x >= 0 der)

    // Tocar fuera de Sprite7 -> Ka (x < 0 izq, x >= 0 der)

    for t in touches() {

        if t.phase == TouchPhase::Started {

            if point_in_scratch_rect(t.position.x, t.position.y, -315.0, 175.0, 90.0, 22.0) {

                continue;

            }

            let (sx, sy) = screen_to_scratch(t.position.x, t.position.y);

            if is_inside_drum(drum_sp, sx, sy) {

                if sx < 0.0 {

                    st.don_left = true;

                } else {

                    st.don_right = true;

                }

            } else if sx < 0.0 {

                st.ka_left = true;

            } else {

                st.ka_right = true;

            }

        }

    }



    st.don_hit = st.don_left || st.don_right;

    st.ka_hit = st.ka_left || st.ka_right;

    st

}



fn log_android(msg: &str) { eprintln!("TAIKO_LOG: {}", msg); }

fn window_conf() -> Conf {

    Conf {

        window_title: "Taiko TJA Simulator (Rust Nativo)".to_string(),

        window_width: 960,

        window_height: 540,

        window_resizable: true,

        ..Default::default()

    }

}



#[macroquad::main(window_conf)]

async fn main() {

    #[cfg(target_os = "android")]

    {

        let _ = fs::create_dir_all(ANDROID_APP_FILES_DIR);

    }



    let font_bytes = load_asset_bytes("font.ttf").await;

    let font = font_bytes

        .as_deref()

        .and_then(|b| load_ttf_font_from_bytes(b).ok());



    let don_bytes = load_asset_bytes("don.wav").await;

    let ka_bytes = load_asset_bytes("ka.wav").await;

    let balloon_bytes = load_asset_bytes("balloon.ogg").await;



    let assets = SvgAssets::load(font_bytes);

    let mats = RenderMaterials::new();



    let mut audio = match AudioEngine::new(don_bytes, ka_bytes, balloon_bytes) {

        Ok(a) => a,

        Err(err) => {

            eprintln!("{}", err);

            return;

        }

    };



    let mut settings = GameSettings::load();

    let mut app_state = AppState::NeedFile;

    let mut current_tja: Option<TjaData> = None;

    let mut session: Option<PlaySession> = None;

    let mut browser: Option<FileBrowserState> = None;

    let mut screenshot_msg_timer: f32 = 0.0;

    let mut screenshot_msg = String::new();



    loop {

        let frame_start = Instant::now();
        let dt = get_frame_time().min(0.05);

        if screenshot_msg_timer > 0.0 {

            screenshot_msg_timer = (screenshot_msg_timer - dt).max(0.0);

        }



        if is_key_pressed(KeyCode::Space) {

            if let Some(tja) = &current_tja {

                let safe_title: String = tja

                    .title

                    .chars()

                    .map(|c| {

                        if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {

                            c

                        } else {

                            '_'

                        }

                    })

                    .collect();

                let filename = format!("{} score.png", safe_title.trim());

                get_screen_data().export_png(&filename);

                screenshot_msg = format!("Captura guardada: {}", filename);

                screenshot_msg_timer = 2.5;

            }

        }



        clear_background(BLACK);



        match app_state {

            AppState::NeedFile => {

                draw_sprite(assets.stage_bg.as_ref(), 0.0, 0.0, 1.0, WHITE);

                draw_text_outlined(

                    "TAIKO TJA SIMULATOR",

                    font.as_ref(),

                    0.0,

                    70.0,

                    32.0,

                    WHITE,

                    BLACK,

                    0.5,

                );

                draw_text_aligned(

                    "Pulsa ENTER o toca para cargar un archivo .tja",

                    font.as_ref(),

                    0.0,

                    20.0,

                    16.0,

                    LIGHTGRAY,

                    0.5,

                );



                let (mx, my) = mouse_position();

                let hover = point_in_scratch_rect(mx, my, -110.0, -5.0, 220.0, 44.0);

                let btn_col = if hover {

                    Color::from_rgba(255, 110, 70, 255)

                } else {

                    Color::from_rgba(230, 75, 45, 255)

                };

                draw_rect_scratch(-110.0, -5.0, 220.0, 44.0, btn_col);

                draw_text_outlined(

                    "CARGAR .TJA",

                    font.as_ref(),

                    0.0,

                    -27.0,

                    18.0,

                    WHITE,

                    BLACK,

                    0.5,

                );



                draw_text_aligned(

                    &audio.sfx_status,

                    font.as_ref(),

                    0.0,

                    -85.0,

                    12.0,

                    SKYBLUE,

                    0.5,

                );

                draw_text_aligned(

                    "PC: Don (F J V N R U) | Ka (D K C M E I) | Táctil: Tambor=Don, Fuera=Ka",

                    font.as_ref(),

                    0.0,

                    -115.0,

                    12.5,

                    GRAY,

                    0.5,

                );



                if is_key_pressed(KeyCode::Enter)

                    || (hover && is_mouse_button_pressed(MouseButton::Left))

                {

                    #[cfg(not(target_os = "android"))]

                    {

                        if let Some(tja) = open_tja_and_audio_desktop(&mut audio) {

                            start_demo_preview(&mut audio, &tja);

                            current_tja = Some(tja);

                            app_state = AppState::SelectDifficulty;

                        }

                    }

                    #[cfg(target_os = "android")]

                    {

                        browser = Some(FileBrowserState::new(

                            BrowserMode::PickTja,

                            BrowserReturn::NeedFile,

                            None,

                        ));

                        app_state = AppState::FileBrowser;

                    }

                }

            }



            AppState::FileBrowser => {

                draw_sprite(assets.stage_bg.as_ref(), 0.0, 0.0, 1.0, WHITE);



                let mut close_to_menu: Option<AppState> = None;

                let mut picked_tja_path: Option<PathBuf> = None;

                let mut picked_audio_path: Option<PathBuf> = None;

                let mut load_demo_tja = false;



                if let Some(br) = &mut browser {

                    let title_txt = match br.mode {

                        BrowserMode::PickTja => "SELECCIONAR ARCHIVO .TJA",

                        BrowserMode::PickAudio => "SELECCIONAR AUDIO (.ogg/.mp3/.wav)",

                    };

                    draw_text_outlined(title_txt, font.as_ref(), 0.0, 158.0, 18.0, WHITE, BLACK, 0.5);



                    let path_str = br.current_dir.to_string_lossy();

                    let short_path = if path_str.len() > 62 {

                        format!("...{}", &path_str[path_str.len() - 59..])

                    } else {

                        path_str.into_owned()

                    };

                    draw_text_aligned(&short_path, font.as_ref(), 0.0, 138.0, 10.5, LIGHTGRAY, 0.5);



                    let (mx, my) = mouse_position();

                    let shortcuts = [

                        ("AppData", ANDROID_APP_FILES_DIR, -295.0),

                        ("TaikoTJA", "/storage/emulated/0/TaikoTJA", -185.0),

                        ("Download", "/storage/emulated/0/Download", -75.0),

                        ("/sdcard", "/storage/emulated/0", 35.0),

                    ];

                    for (lbl, target, bx) in shortcuts {

                        let hov = point_in_scratch_rect(mx, my, bx, 122.0, 102.0, 24.0);

                        draw_rect_scratch(

                            bx,

                            122.0,

                            102.0,

                            24.0,

                            if hov {

                                Color::from_rgba(80, 140, 220, 255)

                            } else {

                                Color::from_rgba(50, 75, 115, 255)

                            },

                        );

                        draw_text_outlined(lbl, font.as_ref(), bx + 51.0, 110.0, 11.5, WHITE, BLACK, 0.5);

                        if hov && is_mouse_button_pressed(MouseButton::Left) {

                            br.current_dir = PathBuf::from(target);

                            br.refresh();

                        }

                    }



                    // Botón demo.tja o "Sin audio"

                    let right_lbl = match br.mode {

                        BrowserMode::PickTja => "demo.tja",

                        BrowserMode::PickAudio => "Sin audio",

                    };

                    let hov_right = point_in_scratch_rect(mx, my, 145.0, 122.0, 82.0, 24.0);

                    draw_rect_scratch(

                        145.0,

                        122.0,

                        82.0,

                        24.0,

                        if hov_right { ORANGE } else { Color::from_rgba(180, 95, 35, 255) },

                    );

                    draw_text_outlined(right_lbl, font.as_ref(), 186.0, 110.0, 11.0, WHITE, BLACK, 0.5);

                    if hov_right && is_mouse_button_pressed(MouseButton::Left) {

                        match br.mode {

                            BrowserMode::PickTja => load_demo_tja = true,

                            BrowserMode::PickAudio => {

                                close_to_menu = Some(AppState::SelectDifficulty);

                            }

                        }

                    }



                    // Botón Volver

                    let hov_back = point_in_scratch_rect(mx, my, 235.0, 122.0, 65.0, 24.0);

                    draw_rect_scratch(

                        235.0,

                        122.0,

                        65.0,

                        24.0,

                        if hov_back { RED } else { Color::from_rgba(165, 45, 45, 255) },

                    );

                    draw_text_outlined("Volver", font.as_ref(), 267.5, 110.0, 11.0, WHITE, BLACK, 0.5);

                    if hov_back && is_mouse_button_pressed(MouseButton::Left) {

                        close_to_menu = Some(match br.return_to {

                            BrowserReturn::NeedFile => AppState::NeedFile,

                            BrowserReturn::SelectDifficulty => AppState::SelectDifficulty,

                        });

                    }



                    // Scroll por rueda o arrastre táctil

                    let (_, wheel_y) = mouse_wheel();

                    if wheel_y.abs() > 0.01 {

                        br.scroll_y -= wheel_y * 24.0;

                    }

                    let list_top = 90.0_f32;

                    let list_bottom = -160.0_f32;

                    let row_h = 30.0_f32;

                    let visible_h = list_top - list_bottom;

                    let max_scroll = ((br.entries.len() as f32) * row_h - visible_h).max(0.0);



                    let mut tapped_pos: Option<(f32, f32)> = None;

                    for t in touches() {

                        match t.phase {

                            TouchPhase::Started => {

                                let (_, sy) = screen_to_scratch(t.position.x, t.position.y);

                                if sy <= list_top && sy >= list_bottom {

                                    br.drag_active = true;

                                    br.drag_start_y = sy;

                                    br.last_touch_y = sy;

                                    br.drag_moved = false;

                                }

                            }

                            TouchPhase::Moved => {

                                if br.drag_active {

                                    let (_, sy) = screen_to_scratch(t.position.x, t.position.y);

                                    let dy = sy - br.last_touch_y;

                                    if (sy - br.drag_start_y).abs() > 8.0 {

                                        br.drag_moved = true;

                                    }

                                    br.scroll_y += dy;

                                    br.last_touch_y = sy;

                                }

                            }

                            TouchPhase::Ended | TouchPhase::Cancelled => {

                                if br.drag_active {

                                    if !br.drag_moved && t.phase == TouchPhase::Ended {

                                        tapped_pos = Some((t.position.x, t.position.y));

                                    }

                                    br.drag_active = false;

                                }

                            }

                            _ => {}

                        }

                    }

                    br.scroll_y = br.scroll_y.clamp(0.0, max_scroll);



                    draw_rect_scratch(-300.0, list_top, 600.0, visible_h, Color::new(0.06, 0.07, 0.1, 0.85));



                    let mut click_entry: Option<usize> = None;

                    for (idx, entry) in br.entries.iter().enumerate() {

                        let y_top = list_top - (idx as f32) * row_h + br.scroll_y;

                        let y_bot = y_top - row_h;

                        if y_bot > list_top || y_top < list_bottom {

                            continue;

                        }

                        let clamped_top = y_top.min(list_top);

                        let clamped_bot = y_bot.max(list_bottom);

                        let ch = clamped_top - clamped_bot;

                        if ch < 8.0 {

                            continue;

                        }



                        let hov = point_in_scratch_rect(mx, my, -296.0, clamped_top, 592.0, ch);

                        let bg = if entry.is_dir {

                            if hov {

                                Color::from_rgba(70, 85, 115, 255)

                            } else {

                                Color::from_rgba(45, 55, 78, 255)

                            }

                        } else if hov {

                            Color::from_rgba(220, 95, 55, 255)

                        } else {

                            Color::from_rgba(75, 45, 65, 255)

                        };

                        draw_rect_scratch(-296.0, clamped_top - 1.0, 592.0, ch - 2.0, bg);



                        let prefix = if entry.is_dir { "[DIR] " } else { "[FILE] " };

                        let label = format!("{}{}", prefix, entry.name);

                        let y_mid = (clamped_top + clamped_bot) * 0.5;

                        draw_text_aligned(&label, font.as_ref(), -286.0, y_mid, 13.0, WHITE, 0.0);



                        if let Some((tx, ty)) = tapped_pos {

                            if point_in_scratch_rect(tx, ty, -296.0, clamped_top, 592.0, ch) {

                                click_entry = Some(idx);

                            }

                        } else if !br.drag_active

                            && hov

                            && is_mouse_button_pressed(MouseButton::Left)

                            && touches().is_empty()

                        {

                            click_entry = Some(idx);

                        }

                    }



                    if let Some(err) = &br.error_msg {

                        draw_text_aligned(err, font.as_ref(), 0.0, -40.0, 12.0, YELLOW, 0.5);

                    }



                    if let Some(idx) = click_entry {

                        let ent = &br.entries[idx];

                        if ent.is_dir {

                            br.current_dir = ent.path.clone();

                            br.refresh();

                        } else {

                            match br.mode {

                                BrowserMode::PickTja => picked_tja_path = Some(ent.path.clone()),

                                BrowserMode::PickAudio => picked_audio_path = Some(ent.path.clone()),

                            }

                        }

                    }

                }



                if load_demo_tja {

                    if let Some(bytes) = load_asset_bytes("demo.tja").await {

                        let text = decode_tja_bytes(&bytes);

                        let tja = parse_tja(&text, None);

                        audio.clear_song();

                        if !tja.wave.is_empty() {

                            if let Some(ab) = load_asset_bytes(&tja.wave).await {

                                if let Err(e) = audio.load_song_bytes(tja.wave.clone(), ab) {

                                    audio.last_audio_error = Some(e);

                                }

                            }

                        }

                        start_demo_preview(&mut audio, &tja);

                        current_tja = Some(tja);

                        browser = None;

                        app_state = AppState::SelectDifficulty;

                    } else if let Some(br) = &mut browser {

                        br.error_msg = Some("No hay demo.tja empaquetado en assets/".to_string());

                    }

                } else if let Some(tpath) = picked_tja_path {

                    if let Ok(bytes) = fs::read(&tpath) {

                        let text = decode_tja_bytes(&bytes);

                        let tja_dir = tpath.parent().map(|p| p.to_path_buf());

                        let tja = parse_tja(&text, tja_dir.clone());

                        audio.clear_song();



                        if let Some(apath) = find_tja_audio(&tja) {

                            if let Err(e) = audio.load_song_from_path(&apath) {

                                audio.last_audio_error = Some(e);

                            }

                            start_demo_preview(&mut audio, &tja);

                            current_tja = Some(tja);

                            browser = None;

                            app_state = AppState::SelectDifficulty;

                        } else {

                            current_tja = Some(tja);

                            browser = Some(FileBrowserState::new(

                                BrowserMode::PickAudio,

                                BrowserReturn::SelectDifficulty,

                                tja_dir,

                            ));

                        }

                    } else if let Some(br) = &mut browser {

                        br.error_msg = Some("No se pudo leer el archivo .tja".to_string());

                    }

                } else if let Some(apath) = picked_audio_path {

                    if let Err(e) = audio.load_song_from_path(&apath) {

                        audio.last_audio_error = Some(e);

                    }

                    if let Some(tja) = &current_tja {

                        start_demo_preview(&mut audio, tja);

                    }

                    browser = None;

                    app_state = AppState::SelectDifficulty;

                } else if let Some(next_st) = close_to_menu {

                    browser = None;

                    app_state = next_st;

                }

            }



            AppState::SelectDifficulty => {
            // demo loop restart
            if let Some(tja)=&current_tja { if audio.song_bytes.is_some() { let done=audio.song_sink.as_ref().map(|s|s.empty()).unwrap_or(true); if done { let _=audio.start_song(tja.demo_start, tja.song_vol); } } }

                let mut start_course: Option<Course> = None;

                let mut go_settings = false;

                let mut reload_file = false;

                let mut change_audio = false;



                draw_sprite(assets.stage_bg.as_ref(), 0.0, 0.0, 1.0, WHITE);



                if let Some(tja) = &current_tja {

                    draw_text_outlined(

                        &tja.title,

                        font.as_ref(),

                        0.0,

                        148.0,

                        22.0,

                        WHITE,

                        BLACK,

                        0.5,

                    );

                    draw_text_aligned(

                        &format!(

                            "BPM: {:.1} | Offset: {:.3}s | Audio: {} | {}",

                            tja.bpm, tja.offset, audio.song_name, audio.sfx_status

                        ),

                        font.as_ref(),

                        0.0,

                        122.0,

                        11.5,

                        if audio.song_bytes.is_some() {

                            LIGHTGRAY

                        } else {

                            ORANGE

                        },

                        0.5,

                    );



                    if let Some(err) = &audio.last_audio_error {

                        draw_text_aligned(err, font.as_ref(), 0.0, 104.0, 11.0, RED, 0.5);

                    }



                    let (mx, my) = mouse_position();

                    let mut y = 85.0;

                    for course in Course::ALL {

                        if let Some(cd) = tja.courses.get(&course) {

                            if cd.exists {

                                let hover =

                                    point_in_scratch_rect(mx, my, -155.0, y, 310.0, 32.0);

                                let bg = if hover {

                                    Color::new(

                                        (course.color().r + 0.15).min(1.0),

                                        (course.color().g + 0.15).min(1.0),

                                        (course.color().b + 0.15).min(1.0),

                                        1.0,

                                    )

                                } else {

                                    course.color()

                                };

                                draw_rect_scratch(-155.0, y, 310.0, 32.0, bg);

                                let diff_icon = match course {

                                    Course::Kantan => assets.diff_kantan.as_ref(),

                                    Course::Futsuu => assets.diff_futsuu.as_ref(),

                                    Course::Muzukashii => assets.diff_muzukashii.as_ref(),

                                    Course::Oni => assets.diff_oni.as_ref(),

                                    Course::UraOni => assets.diff_ura.as_ref(),

                                };

                                draw_sprite(diff_icon, -135.0, y - 16.0, 0.65, WHITE);



                                let label = format!(

                                    "({}) {}   ★{:.0}   [{} notas]",

                                    course.key_hint(),

                                    course.name(),

                                    cd.level,

                                    cd.note_count

                                );

                                draw_text_outlined(

                                    &label,

                                    font.as_ref(),

                                    10.0,

                                    y - 16.0,

                                    15.0,

                                    WHITE,

                                    BLACK,

                                    0.5,

                                );



                                if hover && is_mouse_button_pressed(MouseButton::Left) {

                                    start_course = Some(course);

                                }

                                y -= 38.0;

                            }

                        }

                    }



                    if is_key_pressed(KeyCode::Key1) && tja.courses[&Course::Kantan].exists {

                        start_course = Some(Course::Kantan);

                    }

                    if is_key_pressed(KeyCode::Key2) && tja.courses[&Course::Futsuu].exists {

                        start_course = Some(Course::Futsuu);

                    }

                    if is_key_pressed(KeyCode::Key3) && tja.courses[&Course::Muzukashii].exists {

                        start_course = Some(Course::Muzukashii);

                    }

                    if is_key_pressed(KeyCode::Key4) && tja.courses[&Course::Oni].exists {

                        start_course = Some(Course::Oni);

                    }

                    if is_key_pressed(KeyCode::Key5) && tja.courses[&Course::UraOni].exists {

                        start_course = Some(Course::UraOni);

                    }



                    let hover_s = point_in_scratch_rect(mx, my, -225.0, -125.0, 140.0, 32.0);

                    let hover_a = point_in_scratch_rect(mx, my, -70.0, -125.0, 140.0, 32.0);

                    let hover_l = point_in_scratch_rect(mx, my, 85.0, -125.0, 140.0, 32.0);



                    draw_rect_scratch(

                        -225.0,

                        -125.0,

                        140.0,

                        32.0,

                        if hover_s { GRAY } else { DARKGRAY },

                    );

                    draw_text_outlined(

                        "(S) Ajustes",

                        font.as_ref(),

                        -155.0,

                        -141.0,

                        13.5,

                        WHITE,

                        BLACK,

                        0.5,

                    );



                    draw_rect_scratch(

                        -70.0,

                        -125.0,

                        140.0,

                        32.0,

                        if hover_a {

                            Color::from_rgba(80, 145, 220, 255)

                        } else {

                            Color::from_rgba(45, 100, 175, 255)

                        },

                    );

                    draw_text_outlined(

                        "(A) Cargar Audio",

                        font.as_ref(),

                        0.0,

                        -141.0,

                        13.5,

                        WHITE,

                        BLACK,

                        0.5,

                    );



                    draw_rect_scratch(

                        85.0,

                        -125.0,

                        140.0,

                        32.0,

                        if hover_l { GRAY } else { DARKGRAY },

                    );

                    draw_text_outlined(

                        "(L) Otro .TJA",

                        font.as_ref(),

                        155.0,

                        -141.0,

                        13.5,

                        WHITE,

                        BLACK,

                        0.5,

                    );



                    if is_key_pressed(KeyCode::S)

                        || (hover_s && is_mouse_button_pressed(MouseButton::Left))

                    {

                        go_settings = true;

                    }

                    if is_key_pressed(KeyCode::A)

                        || (hover_a && is_mouse_button_pressed(MouseButton::Left))

                    {

                        change_audio = true;

                    }

                    if is_key_pressed(KeyCode::L)

                        || (hover_l && is_mouse_button_pressed(MouseButton::Left))

                    {

                        reload_file = true;

                    }

                }



                if go_settings {

                    app_state = AppState::SettingsMenu;

                } else if change_audio {

                    #[cfg(not(target_os = "android"))]

                    {

                        if let Some(tja) = &current_tja {

                            pick_audio_manually(&mut audio, tja.tja_dir.as_ref());

                            start_demo_preview(&mut audio, tja);

                        }

                    }

                    #[cfg(target_os = "android")]

                    {

                        let dir = current_tja.as_ref().and_then(|t| t.tja_dir.clone());

                        browser = Some(FileBrowserState::new(

                            BrowserMode::PickAudio,

                            BrowserReturn::SelectDifficulty,

                            dir,

                        ));

                        app_state = AppState::FileBrowser;

                    }

                } else if reload_file {

                    audio.stop_song();

                    #[cfg(not(target_os = "android"))]

                    {

                        if let Some(tja) = open_tja_and_audio_desktop(&mut audio) {

                            start_demo_preview(&mut audio, &tja);

                            current_tja = Some(tja);

                        }

                    }

                    #[cfg(target_os = "android")]

                    {

                        let dir = current_tja.as_ref().and_then(|t| t.tja_dir.clone());

                        browser = Some(FileBrowserState::new(

                            BrowserMode::PickTja,

                            BrowserReturn::SelectDifficulty,

                            dir,

                        ));

                        app_state = AppState::FileBrowser;

                    }

                } else if let Some(course) = start_course {

                    if let Some(tja) = &current_tja {

                        audio.stop_song();

                        let sess = PlaySession::new(tja, course, &settings);

                        if audio.song_bytes.is_some() {

                            if let Err(e) = audio.start_song(0.0, tja.song_vol) {

                                audio.last_audio_error = Some(e);

                            }

                        }

                        session = Some(sess);

                        app_state = AppState::Playing;

                    }

                }

            }



            AppState::SettingsMenu => {

                draw_sprite(assets.stage_bg.as_ref(), 0.0, 0.0, 1.0, WHITE);

                draw_text_outlined(

                    "AJUSTES",

                    font.as_ref(),

                    0.0,

                    130.0,

                    24.0,

                    WHITE,

                    BLACK,

                    0.5,

                );



                let (mx, my) = mouse_position();

                let items = [

                    (

                        "1. Perfect Good Mode (良):",

                        if settings.pgood_mode { "ON" } else { "OFF" },

                        86.0,

                    ),

                    (

                        "2. Early / Late Tilt:",

                        if settings.e_and_l_tilt { "ON" } else { "OFF" },

                        51.0,

                    ),

                    (

                        "3. Auto Mode:",

                        if settings.auto_mode { "ON" } else { "OFF" },

                        16.0,

                    ),

                    (

                        "4. Hitsounds:",

                        if settings.hitsound { "ON" } else { "OFF" },

                        -19.0,

                    ),

                    (

                        "5. Separate Misses from Bads:",

                        if settings.separate_misses { "ON" } else { "OFF" },

                        -54.0,

                    ),

                ];



                for (idx, (label, val, y)) in items.iter().enumerate() {

                    let hover = point_in_scratch_rect(mx, my, -170.0, *y, 340.0, 32.0);

                    let on = *val == "ON";

                    let bg = if hover {

                        Color::from_rgba(90, 95, 120, 255)

                    } else {

                        Color::from_rgba(50, 52, 70, 255)

                    };

                    draw_rect_scratch(-170.0, *y, 340.0, 32.0, bg);

                    draw_text_aligned(label, font.as_ref(), -155.0, *y - 16.0, 15.0, WHITE, 0.0);

                    draw_text_outlined(

                        val,

                        font.as_ref(),

                        155.0,

                        *y - 16.0,

                        15.0,

                        if on { GREEN } else { RED },

                        BLACK,

                        1.0,

                    );



                    let key_hit = match idx {

                        0 => is_key_pressed(KeyCode::Key1),

                        1 => is_key_pressed(KeyCode::Key2),

                        2 => is_key_pressed(KeyCode::Key3),

                        3 => is_key_pressed(KeyCode::Key4),

                        4 => is_key_pressed(KeyCode::Key5),

                        _ => false,

                    };

                    if key_hit || (hover && is_mouse_button_pressed(MouseButton::Left)) {

                        match idx {

                            0 => settings.pgood_mode = !settings.pgood_mode,

                            1 => settings.e_and_l_tilt = !settings.e_and_l_tilt,

                            2 => settings.auto_mode = !settings.auto_mode,

                            3 => settings.hitsound = !settings.hitsound,

                            4 => settings.separate_misses = !settings.separate_misses,

                            _ => {}

                        }

                        settings.save();

                    }

                }



                draw_rect_scratch(-170.0, -94.0, 340.0, 32.0, Color::from_rgba(50, 52, 70, 255));

                draw_text_aligned(

                    "User Offset (- / +):",

                    font.as_ref(),

                    -155.0,

                    -110.0,

                    15.0,

                    WHITE,

                    0.0,

                );

                let hover_minus = point_in_scratch_rect(mx, my, 45.0, -94.0, 28.0, 32.0);

                let hover_plus = point_in_scratch_rect(mx, my, 138.0, -94.0, 28.0, 32.0);

                draw_rect_scratch(

                    45.0,

                    -94.0,

                    28.0,

                    32.0,

                    if hover_minus { GRAY } else { DARKGRAY },

                );

                draw_text_outlined("-", font.as_ref(), 59.0, -110.0, 16.0, WHITE, BLACK, 0.5);

                draw_text_outlined(

                    &format!("{:+.3}s", settings.user_offset),

                    font.as_ref(),

                    105.0,

                    -110.0,

                    14.0,

                    YELLOW,

                    BLACK,

                    0.5,

                );

                draw_rect_scratch(

                    138.0,

                    -94.0,

                    28.0,

                    32.0,

                    if hover_plus { GRAY } else { DARKGRAY },

                );

                draw_text_outlined("+", font.as_ref(), 152.0, -110.0, 16.0, WHITE, BLACK, 0.5);



                if is_key_pressed(KeyCode::Left)

                    || (hover_minus && is_mouse_button_pressed(MouseButton::Left))

                {

                    settings.user_offset -= 0.005;

                    settings.save();

                }

                if is_key_pressed(KeyCode::Right)

                    || (hover_plus && is_mouse_button_pressed(MouseButton::Left))

                {

                    settings.user_offset += 0.005;

                    settings.save();

                }



                let hover_back = point_in_scratch_rect(mx, my, -80.0, -148.0, 160.0, 32.0);

                draw_rect_scratch(

                    -80.0,

                    -148.0,

                    160.0,

                    32.0,

                    if hover_back {

                        Color::from_rgba(80, 160, 240, 255)

                    } else {

                        Color::from_rgba(50, 120, 200, 255)

                    },

                );

                draw_text_outlined(

                    "Volver (ESC)",

                    font.as_ref(),

                    0.0,

                    -164.0,

                    15.0,

                    WHITE,

                    BLACK,

                    0.5,

                );

                if is_key_pressed(KeyCode::Escape)

                    || is_key_pressed(KeyCode::Enter)

                    || (hover_back && is_mouse_button_pressed(MouseButton::Left))

                {

                    settings.save();

                    app_state = AppState::SelectDifficulty;

                }

            }



            AppState::Playing => {

                let mut back_to_menu = false;

                if let (Some(tja), Some(sess)) = (&current_tja, &mut session) {

                    let has_audio_playing = audio.is_playing();

                    let current_time = if has_audio_playing {

                        audio.update_and_get_time()

                    } else {

                        sess.fallback_clock += dt as f64;

                        sess.fallback_clock

                    };



                    if settings.auto_mode {

                        sess.auto_roll_cd = (sess.auto_roll_cd - dt * 60.0).max(0.0);

                        sess.auto_balloon_cd = (sess.auto_balloon_cd - dt * 60.0).max(0.0);

                    }



                    let course_measures = tja

                        .courses

                        .get(&sess.course)

                        .map(|c| c.measures.as_slice())

                        .unwrap_or(&[]);



                    let mut measures_gen = 0;

                    while sess.compas_actual < course_measures.len()

                        && measures_gen < 64

                        && sess.notes.len() < 4000

                    {

                        let tiempo_restante = sess.tiempo_actual - current_time;

                        let vel_global = (sess.velocidad_scroll / 160.0) * sess.bpm_actual;

                        let tiempo_dinamico = if vel_global > 0.0 {

                            (500.0 / vel_global + 1.5).clamp(3.0, f64::INFINITY)

                        } else {

                            3.0

                        };

                        if tiempo_restante < tiempo_dinamico {

                            let m_text = &course_measures[sess.compas_actual];

                            sess.generar_compas(m_text);

                            sess.compas_actual += 1;

                            measures_gen += 1;

                        } else {

                            break;

                        }

                    }



                    while sess.gogo_idx < sess.gogo_events.len()

                        && current_time >= sess.gogo_events[sess.gogo_idx].0

                    {

                        sess.gogo_active = sess.gogo_events[sess.gogo_idx].1;

                        sess.gogo_idx += 1;

                    }

                    while sess.lyric_idx < sess.lyric_events.len()

                        && current_time >= sess.lyric_events[sess.lyric_idx].0

                    {

                        sess.lyric_actual = sess.lyric_events[sess.lyric_idx].1.clone();

                        sess.lyric_idx += 1;

                    }

                    while sess.bpm_idx < sess.bpm_events.len()

                        && current_time >= sess.bpm_events[sess.bpm_idx].0

                    {

                        sess.bpm_display = sess.bpm_events[sess.bpm_idx].1;

                        sess.bpm_idx += 1;

                    }



                    let inp = poll_drum_input(assets.drum_playfield.as_ref());

                    let mut don_hit = inp.don_hit;

                    let mut ka_hit = inp.ka_hit;

                    if settings.auto_mode {

                        don_hit = false;

                        ka_hit = false;

                    } else {

                        if don_hit {

                            sess.drum_flash.drum_bounce = 1.0;

                            sess.drum_flash.lane_flash_kind = Some(true);

                            sess.drum_flash.lane_flash_alpha = 1.0;
                                sess.drum_flash.lane_flash_age = 0.0;

                            if inp.don_left {

                                sess.drum_flash.don_l = 1.0;

                                sess.drum_flash.don_l_age = 0.0;

                            }

                            if inp.don_right {

                                sess.drum_flash.don_r = 1.0;

                                sess.drum_flash.don_r_age = 0.0;

                            }

                            if settings.hitsound {

                                audio.play_don(tja.se_vol);

                            }

                        }

                        if ka_hit {

                            sess.drum_flash.drum_bounce = 1.0;

                            sess.drum_flash.lane_flash_kind = Some(false);

                            sess.drum_flash.lane_flash_alpha = 1.0;
                                sess.drum_flash.lane_flash_age = 0.0;

                            if inp.ka_left {

                                sess.drum_flash.ka_l = 1.0;

                                sess.drum_flash.ka_l_age = 0.0;

                            }

                            if inp.ka_right {

                                sess.drum_flash.ka_r = 1.0;

                                sess.drum_flash.ka_r_age = 0.0;

                            }

                            if settings.hitsound {

                                audio.play_ka(tja.se_vol);

                            }

                        }

                    }



                    let mut input_consumido = settings.auto_mode;

                    let is_easy_normal = sess.course.is_easy_or_normal();

                    let max_window = if is_easy_normal { 0.125 } else { 0.108 };



                    let mut triggered_hits: Vec<(usize, JudgeKind, f64)> = Vec::new();

                    let mut triggered_auto: Vec<usize> = Vec::new();

                    let mut triggered_roll_hits: Vec<(usize, bool)> = Vec::new();

                    let mut popped_balloons: Vec<usize> = Vec::new();

                    // Official auto rates: drumroll 15 hits/s (every 4 frames),
                    // balloon 30 hits/s (every 2 frames).
                    let auto_roll_tick = settings.auto_mode && sess.auto_roll_cd <= 0.0;
                    let auto_balloon_tick = settings.auto_mode && sess.auto_balloon_cd <= 0.0;
                    let mut did_auto_roll = false;
                    let mut did_auto_balloon = false;
                    let mut auto_roll_sound_played = false;



                    for (idx, note) in sess.notes.iter_mut().enumerate() {

                        if note.kind.is_barline() {

                            continue;

                        }

                        // Always advance position, even for Miss/Finished notes, so missed
                        // notes keep scrolling past the judge line until the left border.
                        let mut dist = (note.hit_time - current_time) * note.velocity;

                        let hit_fin = note

                            .roll_id

                            .and_then(|rid| sess.redoble_fin.get(rid).copied().flatten())

                            .unwrap_or(999_999.0);



                        if note.kind.is_balloon()

                            && note.state == NoteState::Active

                            && dist < 0.0

                            && current_time < hit_fin

                            && note.hits < note.hits_needed

                        {

                            dist = 0.0;

                        }

                        note.x_pos = X_JUEZ + dist;



                        if note.state != NoteState::Active {

                            continue;

                        }



                        if note.kind.is_normal() {

                            if settings.auto_mode {

                                if note.hit_time <= current_time {

                                    note.state = NoteState::Hit;

                                    triggered_auto.push(idx);

                                }

                            } else {

                                let err = note.hit_time - current_time;

                                // Only actual hit attempts inside the judgement window count;
                                // late presses beyond it must not generate a Bad.
                                if (-max_window..=max_window).contains(&err) && !input_consumido {

                                    let matches_key = if note.kind.is_don() { don_hit } else { ka_hit };

                                    if matches_key {

                                        input_consumido = true;

                                        let judge = if is_easy_normal {

                                            if (-0.020..=0.020).contains(&err) {

                                                JudgeKind::PGood

                                            } else if (-0.042..=0.042).contains(&err) {

                                                JudgeKind::Good

                                            } else if (-0.108..=0.108).contains(&err) {

                                                JudgeKind::Ok

                                            } else {

                                                JudgeKind::Bad

                                            }

                                        } else if (-0.012..=0.012).contains(&err) {

                                            JudgeKind::PGood

                                        } else if (-0.025..=0.025).contains(&err) {

                                            JudgeKind::Good

                                        } else if (-0.075..=0.075).contains(&err) {

                                            JudgeKind::Ok

                                        } else {

                                            JudgeKind::Bad

                                        };

                                        note.state = if judge == JudgeKind::Bad {

                                            NoteState::Miss

                                        } else {

                                            NoteState::Hit

                                        };

                                        let tilt = if judge == JudgeKind::PGood

                                            || (judge == JudgeKind::Good

                                                && !(settings.pgood_mode && settings.e_and_l_tilt))

                                            || (judge == JudgeKind::Ok && !settings.e_and_l_tilt)

                                        {

                                            0.0

                                        } else {

                                            err

                                        };

                                        triggered_hits.push((idx, judge, tilt));

                                    }

                                }

                                if note.state == NoteState::Active && current_time > note.hit_time + max_window {
                                    // Passive miss: count silently, keep scrolling off-screen.
                                    note.state = NoteState::Miss;
                                    if settings.separate_misses {
                                        sess.miss += 1;
                                    } else {
                                        sess.bad += 1;
                                    }
                                    sess.combo = 0;
                                    sess.tamashii = (sess.tamashii - sess.tamashii_add * 2.0).clamp(0.0, 10000.0);
                                }

                            }

                        } else if current_time >= note.hit_time && current_time <= hit_fin {

                            let auto_tick = if note.kind.is_balloon() {

                                auto_balloon_tick

                            } else {

                                auto_roll_tick

                            };

                            let valid_hit = if note.kind.is_balloon() {

                                (don_hit && !input_consumido) || auto_tick

                            } else {

                                ((don_hit || ka_hit) && !input_consumido) || auto_tick

                            };

                            if valid_hit {

                                if !auto_tick {

                                    input_consumido = true;

                                }

                                note.hits += 1;

                                triggered_roll_hits.push((idx, if auto_tick { true } else { don_hit }));

                                if auto_tick {

                                    flash_auto_side(&mut sess.drum_flash, true);

                                    if note.kind.is_balloon() {

                                        did_auto_balloon = true;

                                    } else {

                                        did_auto_roll = true;

                                    }

                                    if settings.hitsound && !auto_roll_sound_played {

                                        audio.play_don(tja.se_vol);

                                        auto_roll_sound_played = true;

                                    }

                                }

                                if note.kind.is_balloon() && note.hits >= note.hits_needed {

                                    note.state = NoteState::Hit;

                                    popped_balloons.push(idx);

                                }

                            }

                        } else if current_time > hit_fin {

                            note.state = NoteState::Finished;

                            if note.kind.is_balloon() {

                                sess.globo_restante = 0;

                            }

                        }

                    }



                    if did_auto_roll {

                        sess.auto_roll_cd = 4.0;

                    }

                    if did_auto_balloon {

                        sess.auto_balloon_cd = 2.0;

                    }



                    let mut auto_don_played = false;

                    let mut auto_ka_played = false;

                    for idx in triggered_auto {

                        let kind = sess.notes[idx].kind;

                        // Auto play alternates sides like the official games.
                        flash_auto_side(&mut sess.drum_flash, kind.is_don());

                        if settings.hitsound {

                            if kind.is_don() && !auto_don_played {

                                audio.play_don(tja.se_vol);

                                auto_don_played = true;

                            } else if !kind.is_don() && !auto_ka_played {

                                audio.play_ka(tja.se_vol);

                                auto_ka_played = true;

                            }

                        }

                        sess.register_judge(JudgeKind::PGood, 0.0, kind, &settings);

                    }



                    for (idx, judge, tilt) in triggered_hits {

                        let kind = sess.notes[idx].kind;

                        sess.register_judge(judge, tilt, kind, &settings);

                    }



                    // Passive misses (no hit attempt) are not judged as Bad.



                    for (idx, was_don) in triggered_roll_hits {

                        let kind = sess.notes[idx].kind;

                        let pts = match kind {

                            NoteKind::RollS => 100,

                            NoteKind::BigRollS => 200,

                            _ => 300,

                        };

                        sess.add_score(pts);

                        if kind.is_roll() {

                            sess.confetti.push(ConfettiFx {

                                x: rand::gen_range(-360.0, -150.0),

                                y: -205.0,

                                color_fx: rand::gen_range(1, 5) as f32 * 50.0,

                                age_frames: 0.0,

                            });

                            cap_vec_front(&mut sess.confetti, 120);

                            let fly_kind = if was_don {

                                if kind.is_big() {

                                    NoteKind::BigDon

                                } else {

                                    NoteKind::Don

                                }

                            } else if kind.is_big() {

                                NoteKind::BigKa

                            } else {

                                NoteKind::Ka

                            };

                            sess.spawn_flying_note(fly_kind);

                            sess.roll_contador = sess.notes[idx].hits;

                            sess.roll_ultimo_golpe = get_time();

                            sess.roll_pop_anim = 1.0;
                            sess.roll_pop_anim_age = 0.0;

                            sess.roll_flashes.push(RollFlashFx { age_frames: 0.0 });

                            cap_vec_front(&mut sess.roll_flashes, 32);

                        } else if kind.is_balloon() {

                            sess.globo_restante =

                                (sess.notes[idx].hits_needed - sess.notes[idx].hits).max(0);

                            sess.balloon_pop_anim = 1.0;
                                sess.balloon_pop_anim_age = 0.0;

                        }

                    }



                    for idx in popped_balloons {

                        sess.globo_pop_id = sess.notes[idx].roll_id;

                        sess.globo_restante = 0;

                        sess.add_score(5000);

                        if settings.hitsound {

                            audio.play_balloon(tja.se_vol);

                        }

                        sess.balloon_pops.push(BalloonPopFx { age_frames: 0.0 });

                        cap_vec_front(&mut sess.balloon_pops, 16);

                        sess.spawn_flying_note(NoteKind::Balloon);

                    }



                    sess.notes.retain(|n| {
                        let hit_fin = n
                            .roll_id
                            .and_then(|rid| sess.redoble_fin.get(rid).copied().flatten())
                            .unwrap_or(999_999.0);
                        if n.state == NoteState::Hit {
                            return false;
                        }
                        if n.kind.is_normal() {
                            // Keep Active normal notes until the main loop marks them Miss,
                            // even if they are already off-screen at very high scroll speeds.
                            // Missed notes then keep scrolling until they cross the left edge.
                            return n.state == NoteState::Active || n.x_pos >= -370.0;
                        }
                        let end_x = X_JUEZ + (hit_fin - current_time) * n.velocity;
                        // Keep finished rolls/balloons scrolling until they cross the left edge.
                        end_x >= -370.0
                    });



                    update_animations(sess, dt);



                    let all_measures_done = sess.compas_actual >= course_measures.len();
                    let all_notes_resolved = sess.notes.iter().all(|n| n.state != NoteState::Active);

                    let song_finished = !audio.is_playing()
                        && all_measures_done
                        && all_notes_resolved
                        && (audio.song_bytes.is_none() && current_time > sess.tiempo_actual + 1.0
                            || audio.song_bytes.is_some());



                    if song_finished {

                        sess.finish_timer += dt;

                        if sess.finish_timer >= 2.0 && !sess.finished {

                            sess.finished = true;

                            app_state = AppState::Finished;

                        }

                    }



                    draw_gameplay(&assets, &mats, tja, sess, font.as_ref(), &settings, current_time);



                    let (mx, my) = mouse_position();

                    let hover_menu = point_in_scratch_rect(mx, my, -315.0, 175.0, 90.0, 22.0);

                    draw_rect_scratch(

                        -315.0,

                        175.0,

                        90.0,

                        22.0,

                        if hover_menu {

                            Color::new(0.4, 0.4, 0.5, 0.9)

                        } else {

                            Color::new(0.2, 0.2, 0.28, 0.8)

                        },

                    );

                    draw_text_aligned(

                        "ESC: Menú",

                        font.as_ref(),

                        -270.0,

                        164.0,

                        11.0,

                        WHITE,

                        0.5,

                    );

                    if is_key_pressed(KeyCode::Escape)

                        || (hover_menu && is_mouse_button_pressed(MouseButton::Left))

                    {

                        back_to_menu = true;

                    }

                }



                if back_to_menu {

                    audio.stop_song();

                    if let Some(tja) = &current_tja {

                        start_demo_preview(&mut audio, tja);

                    }

                    session = None;

                    app_state = AppState::SelectDifficulty;

                }

            }



            AppState::Finished => {

                if let (Some(tja), Some(sess)) = (&current_tja, &mut session) {

                    let fdt = dt * 60.0;

                    sess.rainbow_scroll = (sess.rainbow_scroll + fdt * 5.0) % 360.0;

                    sess.finish_anim_frames += fdt;

                    update_animations(sess, dt);



                    draw_gameplay(&assets, &mats, tja, sess, font.as_ref(), &settings, 999_999.0);

                    draw_results_overlay(&assets, sess, font.as_ref(), &settings);



                    let (mx, my) = mouse_position();

                    let hover_retry = point_in_scratch_rect(mx, my, -155.0, -110.0, 145.0, 32.0);

                    let hover_menu = point_in_scratch_rect(mx, my, 10.0, -110.0, 145.0, 32.0);



                    draw_rect_scratch(

                        -155.0,

                        -110.0,

                        145.0,

                        32.0,

                        if hover_retry {

                            ORANGE

                        } else {

                            Color::from_rgba(220, 110, 30, 255)

                        },

                    );

                    draw_text_outlined(

                        "Reintentar (R)",

                        font.as_ref(),

                        -82.5,

                        -126.0,

                        14.0,

                        WHITE,

                        BLACK,

                        0.5,

                    );



                    draw_rect_scratch(

                        10.0,

                        -110.0,

                        145.0,

                        32.0,

                        if hover_menu {

                            SKYBLUE

                        } else {

                            Color::from_rgba(45, 130, 210, 255)

                        },

                    );

                    draw_text_outlined(

                        "Menú (ESC)",

                        font.as_ref(),

                        82.5,

                        -126.0,

                        14.0,

                        WHITE,

                        BLACK,

                        0.5,

                    );



                    if is_key_pressed(KeyCode::R)

                        || (hover_retry && is_mouse_button_pressed(MouseButton::Left))

                    {

                        let c = sess.course;

                        audio.stop_song();

                        let new_sess = PlaySession::new(tja, c, &settings);

                        if audio.song_bytes.is_some() {

                            if let Err(e) = audio.start_song(0.0, tja.song_vol) {

                                audio.last_audio_error = Some(e);

                            }

                        }

                        session = Some(new_sess);

                        app_state = AppState::Playing;

                    } else if is_key_pressed(KeyCode::Escape)

                        || is_key_pressed(KeyCode::Enter)

                        || (hover_menu && is_mouse_button_pressed(MouseButton::Left))

                    {

                        audio.stop_song();

                        start_demo_preview(&mut audio, tja);

                        session = None;

                        app_state = AppState::SelectDifficulty;

                    }

                }

            }

        }



        if screenshot_msg_timer > 0.0 {

            draw_text_outlined(

                &screenshot_msg,

                font.as_ref(),

                0.0,

                -165.0,

                13.0,

                YELLOW,

                BLACK,

                0.5,

            );

        }



        let elapsed = frame_start.elapsed();
        let target = Duration::from_millis(16);
        if elapsed < target { std::thread::sleep(target - elapsed); }
        next_frame().await;

    }

}



fn update_animations(sess: &mut PlaySession, dt: f32) {

    let fdt = dt * 60.0;

    sess.rainbow_scroll = (sess.rainbow_scroll + fdt * 5.0) % 360.0;

    if sess.gogo_active {

        sess.gogo_anim = (sess.gogo_anim + fdt / 7.0).min(1.0);

    } else {

        sess.gogo_anim = (sess.gogo_anim - fdt / 5.0).max(0.0);

    }



    sess.soul_flash_timer = (sess.soul_flash_timer + fdt) % 28.0;

    sess.soul_rainbow_color_fx = (sess.soul_rainbow_color_fx - 4.5 * fdt).rem_euclid(200.0);
    if sess.tamashii >= 10000.0 {
        sess.soul_rainbow_fade = (sess.soul_rainbow_fade + fdt / 15.0).min(1.0);
    } else {
        sess.soul_rainbow_fade = (sess.soul_rainbow_fade - fdt / 15.0).max(0.0);
    }

    sess.fire_frame_timer += fdt;

    if sess.fire_frame_timer >= 3.0 {

        sess.fire_frame_timer -= 3.0;

        sess.fire_frame_idx = (sess.fire_frame_idx + 1) % 6;

    }



    sess.drum_flash.don_l_age += fdt;
    sess.drum_flash.don_r_age += fdt;
    sess.drum_flash.ka_l_age += fdt;
    sess.drum_flash.ka_r_age += fdt;
    // hold drum side flashes for 5 frames before fading
    if sess.drum_flash.don_l_age >= 5.0 {
        sess.drum_flash.don_l = (sess.drum_flash.don_l - fdt / 10.0).max(0.0);
    }
    if sess.drum_flash.don_r_age >= 5.0 {
        sess.drum_flash.don_r = (sess.drum_flash.don_r - fdt / 10.0).max(0.0);
    }
    if sess.drum_flash.ka_l_age >= 5.0 {
        sess.drum_flash.ka_l = (sess.drum_flash.ka_l - fdt / 10.0).max(0.0);
    }
    if sess.drum_flash.ka_r_age >= 5.0 {
        sess.drum_flash.ka_r = (sess.drum_flash.ka_r - fdt / 10.0).max(0.0);
    }
    sess.drum_flash.drum_bounce = (sess.drum_flash.drum_bounce - fdt / 10.0).max(0.0);

    sess.drum_flash.lane_flash_age += fdt;
    sess.drum_flash.judge_ring_age += fdt;
    sess.drum_flash.judge_lane_age += fdt;
    // hold then fade
    if sess.drum_flash.lane_flash_age >= 4.0 { sess.drum_flash.lane_flash_alpha = (sess.drum_flash.lane_flash_alpha - fdt / 8.0).max(0.0); }
    if sess.drum_flash.judge_ring_age >= 2.0 { sess.drum_flash.judge_ring_alpha = (sess.drum_flash.judge_ring_alpha - fdt / 23.0).max(0.0); }
    if sess.drum_flash.judge_lane_age >= 4.0 { sess.drum_flash.judge_lane_alpha = (sess.drum_flash.judge_lane_alpha - fdt / 8.0).max(0.0); }



    // number stretch: 100% -> 110% -> 120% on first 3 frames, then -2%/frame to 100%
    fn stretch_anim(age: f32) -> f32 {
        if age < 1.0 { 0.0 }
        else if age < 2.0 { 0.10 }
        else if age < 3.0 { 0.20 }
        else { (0.20 - (age - 3.0) * 0.02).max(0.0) }
    }
    sess.score_anim = stretch_anim(sess.score_anim_age);
    sess.combo_anim = stretch_anim(sess.combo_anim_age);
    sess.roll_pop_anim = stretch_anim(sess.roll_pop_anim_age);
    sess.balloon_pop_anim = stretch_anim(sess.balloon_pop_anim_age);
    sess.score_anim_age += fdt;
    sess.combo_anim_age += fdt;
    sess.roll_pop_anim_age += fdt;
    sess.balloon_pop_anim_age += fdt;



    for p in &mut sess.judge_popups {

        p.age_frames += fdt;

    }

    sess.judge_popups.retain(|p| p.age_frames < 19.0);



    for b in &mut sess.judge_bursts {

        b.age_frames += fdt;

    }

    sess.judge_bursts.retain(|b| b.age_frames < 13.0);



    for s in &mut sess.score_popups {

        s.age_frames += fdt;

    }

    sess.score_popups.retain(|s| {

        let max_len = s.text.chars().filter(|c| c.is_ascii_digit()).count() as f32;

        s.age_frames < 26.0 + max_len

    });



    for f in &mut sess.roll_flashes {

        f.age_frames += fdt;

    }

    sess.roll_flashes.retain(|f| f.age_frames < 16.0);



    for b in &mut sess.balloon_pops {

        b.age_frames += fdt;

    }

    sess.balloon_pops.retain(|b| b.age_frames < 12.0);



    let step_conf = 6.0 * (45.0_f32.to_radians().sin());

    for c in &mut sess.confetti {

        c.age_frames += fdt;

        c.x += step_conf * fdt;

        c.y += step_conf * fdt;

    }

    sess.confetti.retain(|c| c.age_frames < 90.0);



    for d in &mut sess.dancers {

        d.age_frames += fdt;

        d.bpm = sess.bpm_display;
        let step = ((d.bpm as f32 / 120.0) * 4.0) * fdt;

        d.x += step;

        d.y = 120.0 + (d.x * if d.is_miss { 1.0 } else { 1.5 }).to_radians().sin() * 10.0;

        if d.is_miss {

            d.size = (d.age_frames * 10.0).min(50.0);

        } else if d.age_frames < 5.0 {

            d.size = d.age_frames * 6.0;

        } else {

            d.size = 30.0 + d.x.to_radians().sin() * 5.0;

        }

    }

    sess.dancers.retain(|d| d.x <= 380.0);



    let mut new_explosions = Vec::new();

    let mut new_glows = Vec::new();

    for fn_item in &mut sess.flying_notes {

        if !fn_item.exploded {

            fn_item.anim = (fn_item.anim + 4.625 * fdt).min(134.125);

            let target_step = (fn_item.anim / 4.625).floor() as i32;

            while fn_item.frame_count < target_step && fn_item.frame_count < 29 {

                fn_item.frame_count += 1;

                if fn_item.kind.is_big()

                    && fn_item.frame_count % 3 == 0

                    && fn_item.frame_count <= 27

                {

                    let step_anim = (fn_item.frame_count as f32) * 4.625;

                    let rad = (step_anim - 70.0).to_radians();

                    let gx = rad.sin() * 240.0 + 82.0;

                    let gy = rad.cos() * 240.0 - 40.0;

                    new_glows.push(BigGlowTrail {

                        x: gx,

                        y: gy,

                        age_frames: 0.0,

                    });

                }

            }

            if fn_item.anim >= 134.125 {

                fn_item.exploded = true;

                new_explosions.push(ExplosionFx {

                    x: SOUL_X,

                    y: SOUL_Y,

                    size: 22.5,

                    angle: 0.0,

                    color_fx: 33.0,

                    age_frames: 0.0,

                });

            }

        } else {

            fn_item.fade_frames += fdt;

        }

    }

    sess.flying_notes.retain(|f| {

        let max_f = if f.kind.is_balloon() { 30.0 } else { 21.0 };

        f.fade_frames < max_f

    });

    sess.big_glows.extend(new_glows);

    cap_vec_front(&mut sess.big_glows, 300);

    sess.explosions.extend(new_explosions);

    cap_vec_front(&mut sess.explosions, 80);



    for bg in &mut sess.big_glows {

        bg.age_frames += fdt;

    }

    sess.big_glows.retain(|bg| bg.age_frames < 16.0);



    for ex in &mut sess.explosions {

        ex.age_frames += fdt;

        if ex.age_frames >= 2.0 {

            ex.size += (21.0 - ex.size) * (-0.2) * fdt;

            ex.angle += 5.0 * fdt;

            ex.color_fx -= 2.5 * fdt;

        }

    }

    sess.explosions.retain(|ex| ex.age_frames < 17.0);



    for fw in &mut sess.fireworks {

        if fw.delay_frames > 0.0 {

            fw.delay_frames -= fdt;

        } else {

            fw.age_frames += fdt;

        }

    }

    sess.fireworks.retain(|fw| {

        let max_f = if fw.big { 35.0 } else { 25.0 };

        fw.age_frames < max_f

    });

}



fn draw_gameplay(

    assets: &SvgAssets,

    mats: &RenderMaterials,

    tja: &TjaData,

    sess: &PlaySession,

    font: Option<&Font>,

    settings: &GameSettings,

    current_time: f64,

) {

    // 1. Fondo del escenario

    draw_sprite(assets.stage_bg.as_ref(), 0.0, 0.0, 1.0, WHITE);



    // Lote aditivo 1: Fireworks + Llama Gogo + Círculo juez + Destello carril

    mats.additive_fx.set_uniform("u_hue_shift", 0.0_f32);

    mats.additive_fx.set_uniform("u_brightness", 0.0_f32);

    gl_use_material(&mats.additive_fx);



    for fw in &sess.fireworks {

        if fw.delay_frames <= 0.0 {

            let (size_pct, alpha) = if fw.big {

                if fw.age_frames < 20.0 {

                    (fw.age_frames * 5.0, 1.0)

                } else {

                    let t = fw.age_frames - 20.0;

                    (100.0 + t * 2.0, (1.0 - t / 15.0).max(0.0))

                }

            } else if fw.age_frames < 15.0 {

                (fw.age_frames * 5.0, 1.0)

            } else {

                let t = fw.age_frames - 15.0;

                (75.0 + t * 2.0, (1.0 - t / 10.0).max(0.0))

            };

            let col = hsv_to_color(fw.hue, 0.5, 1.0, alpha);

            draw_sprite_ex(

                assets.firework_upper.as_ref(),

                fw.x,

                fw.y,

                size_pct / 100.0,

                size_pct / 100.0,

                0.0,

                col,

            );

        }

    }



    if sess.gogo_anim > 0.0 {

        let frame = assets.gogo_fire.get(sess.fire_frame_idx).and_then(|o| o.as_ref());

        let sc = sess.gogo_anim;

        draw_sprite_ex(

            frame,

            -150.0,

            25.0,

            sc,

            sc,

            0.0,

            Color::new(1.0, 1.0, 1.0, 0.75 * sess.gogo_anim),

        );

    }



    draw_sprite_ex(

        assets.judge_circle.as_ref(),

        X_JUEZ as f32,

        Y_PISTA,

        1.0,

        1.0,

        0.0,

        WHITE,

    );



    if sess.drum_flash.lane_flash_alpha > 0.0 {

        let lane_sp = match sess.drum_flash.lane_flash_kind {

            Some(true) => assets.lane_don.as_ref(),

            _ => assets.lane_ka.as_ref(),

        };

        draw_sprite_ex(

            lane_sp,

            0.0,

            0.0,

            1.0,

            1.0,

            0.0,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.lane_flash_alpha),

        );

    }

    if sess.drum_flash.judge_lane_alpha > 0.0 {

        draw_sprite_ex(

            assets.lane_hit.as_ref(),

            0.0,

            0.0,

            1.0,

            1.0,

            0.0,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.judge_lane_alpha),

        );

    }



    gl_use_default_material();



    if sess.drum_flash.judge_ring_alpha > 0.0 {

        let ring_sp = match sess.drum_flash.judge_ring_kind {

            JudgeKind::PGood => assets.ring_pgood.as_ref(),

            JudgeKind::Good => assets.ring_good.as_ref(),

            JudgeKind::Ok => assets.ring_ok.as_ref(),

            JudgeKind::Bad => None,

        };

        draw_sprite(

            ring_sp,

            X_JUEZ as f32,

            Y_PISTA,

            1.0,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.judge_ring_alpha),

        );

    }



    // 4. Confeti (Sprite29) agrupado en 4 lotes de tono (50, 100, 150, 200)

    if !sess.confetti.is_empty() {

        mats.normal_fx.set_uniform("u_brightness", 0.0_f32);

        for &fx_bucket in &[50.0_f32, 100.0, 150.0, 200.0] {

            let mut has_any = false;

            for c in &sess.confetti {

                if (c.color_fx - fx_bucket).abs() < 1.0 {

                    if !has_any {

                        mats.normal_fx.set_uniform("u_hue_shift", fx_bucket * 1.8);

                        gl_use_material(&mats.normal_fx);

                        has_any = true;

                    }

                    draw_sprite_ex(

                        assets.confetti.as_ref(),

                        c.x,

                        c.y,

                        1.0,

                        1.0,

                        -45.0,

                        WHITE,

                    );

                }

            }

            if has_any {

                gl_use_default_material();

            }

        }

    }



    // 5. Barra de precisión y Rango (Sprite24, Sprite25, Sprite16)

    let perc = sess.accuracy_percent().clamp(0.0, 100.0);

    let rank_col = if perc >= 95.0 {

        Color::from_rgba(128, 255, 255, 255)

    } else if perc >= 90.0 {

        Color::from_rgba(255, 224, 128, 255)

    } else if perc >= 80.0 {

        Color::from_rgba(255, 128, 128, 255)

    } else if perc >= 70.0 {

        Color::from_rgba(255, 0, 0, 255)

    } else if perc >= 55.0 {

        Color::from_rgba(255, 255, 0, 255)

    } else if perc >= 40.0 {

        Color::from_rgba(0, 255, 0, 255)

    } else if perc >= 20.0 {

        Color::from_rgba(0, 255, 255, 255)

    } else {

        Color::from_rgba(0, 102, 255, 255)

    };

    let clip_right = if perc >= 95.0 {

        ((perc - 95.0) / 0.025) * 2.0

    } else if perc >= 90.0 {

        ((perc - 90.0) / 0.05) * 2.0

    } else if perc >= 80.0 {

        ((perc - 80.0) / 0.1) * 2.0

    } else if perc >= 70.0 {

        ((perc - 70.0) / 0.1) * 2.0

    } else if perc >= 55.0 {

        ((perc - 55.0) / 0.15) * 2.0

    } else if perc >= 40.0 {

        ((perc - 40.0) / 0.15) * 2.0

    } else if perc >= 20.0 {

        ((perc - 20.0) / 0.2) * 2.0

    } else {

        (perc / 0.2) * 2.0

    } as f32;

    draw_sprite(assets.perc_bg.as_ref(), 0.0, 110.0, 1.0, rank_col);

    draw_sprite_clipped_x(

        assets.perc_fill.as_ref(),

        0.0,

        110.0,

        1.0,

        1.0,

        0.0,

        clip_right.clamp(0.0, 200.0),

        rank_col,

    );

    if let Some(rank_sp) = assets.ranks.get(sess.rank_label()).and_then(|o| o.as_ref()) {

        draw_sprite(Some(rank_sp), 220.0, 110.0, 1.0, WHITE);

    }



    // 6. Notas activas

    for note in sess.notes.iter().rev() {

        let hit_fin = note

            .roll_id

            .and_then(|rid| sess.redoble_fin.get(rid).copied().flatten())

            .unwrap_or(999_999.0);



        match note.kind {

            NoteKind::RollS | NoteKind::BigRollS => {

                let mut x_ini = note.x_pos;

                let mut x_fin = X_JUEZ + (hit_fin - current_time) * note.velocity;

                if x_ini < -370.0 {

                    x_ini = -370.0;

                }

                if x_fin > 420.0 {

                    x_fin = 420.0;

                }

                let (head, tail) = if note.kind.is_big() {

                    (assets.big_drumroll_s.as_ref(), assets.big_drumroll_e.as_ref())

                } else {

                    (assets.drumroll_s.as_ref(), assets.drumroll_e.as_ref())

                };

                if x_fin < 370.0 {

                    draw_sprite(tail, x_fin as f32, Y_PISTA, 1.0, WHITE);

                }

                if x_fin > x_ini && x_ini < 370.0 {

                    draw_drumroll_body(x_ini as f32, x_fin as f32, Y_PISTA, note.kind.is_big());

                }

                if note.x_pos < 370.0 {

                    draw_sprite(head, note.x_pos as f32, Y_PISTA, 1.0, WHITE);

                }

            }

            NoteKind::Balloon => {

                if note.x_pos < 370.0 {

                    let pop_scale = if current_time >= note.hit_time

                        && current_time < hit_fin

                        && note.hits_needed > 0

                    {

                        let ratio = (note.hits as f32 / note.hits_needed as f32).clamp(0.0, 1.0);

                        1.0 + 0.6 * ratio

                    } else {

                        1.0

                    };

                    draw_sprite(

                        assets.balloon_e.as_ref(),

                        (note.x_pos + GLOBO_OFFSET) as f32,

                        Y_PISTA,

                        pop_scale,

                        WHITE,

                    );

                    draw_sprite(

                        assets.balloon_s.as_ref(),

                        note.x_pos as f32,

                        Y_PISTA,

                        1.0,

                        WHITE,

                    );

                }

            }

                        NoteKind::Barline => {
                let x = note.x_pos as f32;
                if x > -330.0 && x < 330.0 {
                    draw_rect_scratch(x - 1.0, Y_PISTA + 52.0, 2.0, 104.0, Color::new(1.0, 1.0, 1.0, 0.55));
                }
            }

NoteKind::Don => {

                if note.x_pos < 370.0 {

                    draw_sprite(assets.don.as_ref(), note.x_pos as f32, Y_PISTA, 1.0, WHITE);

                }

            }

            NoteKind::Ka => {

                if note.x_pos < 370.0 {

                    draw_sprite(assets.ka.as_ref(), note.x_pos as f32, Y_PISTA, 1.0, WHITE);

                }

            }

            NoteKind::BigDon => {

                if note.x_pos < 370.0 {

                    draw_sprite(assets.big_don.as_ref(), note.x_pos as f32, Y_PISTA, 1.0, WHITE);

                }

            }

            NoteKind::BigKa => {

                if note.x_pos < 370.0 {

                    draw_sprite(assets.big_ka.as_ref(), note.x_pos as f32, Y_PISTA, 1.0, WHITE);

                }

            }

        }

    }



    // 7. Barra Soul (Sprite11, 12, 17, 18) + Fuego 10000 (Sprite14)

    let (soul_bg, soul_fill, soul_flash) = match sess.course {
        Course::Kantan => (assets.soul_bg_easy.as_ref(), assets.soul_fill_easy.as_ref(), assets.soul_clear_flash_easy.as_ref()),
        Course::Futsuu | Course::Muzukashii => (assets.soul_bg_nh.as_ref(), assets.soul_fill_nh.as_ref(), assets.soul_clear_flash_nh.as_ref()),
        Course::Oni | Course::UraOni => (assets.soul_bg.as_ref(), assets.soul_fill.as_ref(), assets.soul_clear_flash.as_ref()),
    };

    draw_sprite(soul_bg, 45.0, 62.0, 1.5, WHITE);

    let segments = (sess.tamashii / 200.0).floor().clamp(0.0, 50.0) as f32;
    let soul_clip_x2 = segments * 2.0 * 3.86 - 101.0;
    draw_sprite_clipped_x(soul_fill, 45.0, 62.0, 1.5, 1.5, -101.0, soul_clip_x2, WHITE);

    if sess.tamashii >= 8000.0 {

        let a = if sess.soul_flash_timer < 4.0 {

            sess.soul_flash_timer / 4.0

        } else if sess.soul_flash_timer < 6.0 {

            1.0

        } else if sess.soul_flash_timer < 10.0 {

            1.0 - (sess.soul_flash_timer - 6.0) / 4.0

        } else {

            0.0

        };

        if a > 0.0 {

            draw_sprite_clipped_x(

                            soul_flash,

                45.0,

                62.0,

                1.5,

                1.5,

                -101.0,

                202.0,

                Color::new(1.0, 1.0, 1.0, a),

            );

        }

    }

    if sess.soul_rainbow_fade > 0.0 {

        draw_sprite_fx(
            mats,
            false,
            sess.soul_rainbow_color_fx * 1.8,
            0.0,
            match sess.course {
                Course::Kantan => assets.soul_rainbow_easy.as_ref(),
                Course::Futsuu | Course::Muzukashii => assets.soul_rainbow_nh.as_ref(),
                Course::Oni | Course::UraOni => assets.soul_rainbow.as_ref(),
            },
            45.0,
            62.0,
            1.5,
            1.5,
            0.0,
            Color::new(1.0, 1.0, 1.0, sess.soul_rainbow_fade),
        );

        let fire_sp = assets.soul_fire.get(sess.fire_frame_idx).and_then(|o| o.as_ref());

        draw_sprite(fire_sp, SOUL_X, SOUL_Y, 0.72, WHITE);

    }



    // 8. Caja roja izquierda (Sprite4) + Tambor (Sprite26, DonL/R, KaL/R) + Dificultad (Sprite6)

    draw_sprite(assets.track_red.as_ref(), 0.0, 25.0, 1.0, WHITE);

    draw_sprite(assets.drum_icon.as_ref(), -235.0, 25.0, 0.5, WHITE);

    if sess.drum_flash.ka_l > 0.0 {

        draw_sprite(

            assets.ka_l.as_ref(),

            -235.0,

            25.0,

            0.5,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.ka_l),

        );

    }

    if sess.drum_flash.ka_r > 0.0 {

        draw_sprite(

            assets.ka_r.as_ref(),

            -235.0,

            25.0,

            0.5,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.ka_r),

        );

    }

    if sess.drum_flash.don_l > 0.0 {

        draw_sprite(

            assets.don_l.as_ref(),

            -235.0,

            25.0,

            0.5,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.don_l),

        );

    }

    if sess.drum_flash.don_r > 0.0 {

        draw_sprite(

            assets.don_r.as_ref(),

            -235.0,

            25.0,

            0.5,

            Color::new(1.0, 1.0, 1.0, sess.drum_flash.don_r),

        );

    }



    let diff_sp = match sess.course {

        Course::Kantan => assets.diff_kantan.as_ref(),

        Course::Futsuu => assets.diff_futsuu.as_ref(),

        Course::Muzukashii => assets.diff_muzukashii.as_ref(),

        Course::Oni => assets.diff_oni.as_ref(),

        Course::UraOni => assets.diff_ura.as_ref(),

    };

    draw_sprite(diff_sp, -295.0, 30.0, 1.0, WHITE);



    // 9. Tambor grande inferior (Sprite7)

    draw_sprite(

        assets.drum_playfield.as_ref(),

        0.0,

        -sess.drum_flash.drum_bounce * 10.0,

        1.0,

        Color::new(1.0, 1.0, 1.0, 0.25),

    );



    // 10. Burbujas de Redoble y Globo

    if sess.roll_contador >= 1 && (get_time() - sess.roll_ultimo_golpe) < 1.0 {

        draw_sprite(assets.roll_bubble.as_ref(), -150.0, 130.0, 1.0, WHITE);

        let stretch_y = 1.0 + sess.roll_pop_anim;

        draw_digits_row(

            &assets.digits,

            &sess.roll_contador.to_string(),

            -150.0,

            135.0 + sess.roll_pop_anim * 2.0,

            180.0,

            stretch_y,

            WHITE,

        );

    }



    if sess.globo_restante >= 1 {

        draw_sprite(assets.balloon_bubble.as_ref(), -100.0, 70.0, 1.0, WHITE);

        let stretch_y = 1.0 + sess.balloon_pop_anim;

        draw_digits_row(

            &assets.digits,

            &sess.globo_restante.to_string(),

            -100.0,

            70.0 + sess.balloon_pop_anim * 2.0,

            125.0,

            stretch_y,

            WHITE,

        );

    }



    // Lote aditivo 2: roll_flashes + balloon_pops + judge_bursts

    if !sess.roll_flashes.is_empty() || !sess.balloon_pops.is_empty() || !sess.judge_bursts.is_empty() {

        mats.additive_fx.set_uniform("u_hue_shift", 0.0_f32);

        mats.additive_fx.set_uniform("u_brightness", 0.0_f32);

        gl_use_material(&mats.additive_fx);



        for rf in &sess.roll_flashes {

            let t = rf.age_frames;

            let sz = if t < 5.0 { 60.0 + t * 5.0 } else { 85.0 + (t - 5.0) * 5.0 };

            let alpha = if t < 5.0 { 1.0 } else { (1.0 - (t - 5.0) / 10.0).max(0.0) };

            draw_sprite_ex(

                assets.roll_flash.as_ref(),

                X_JUEZ as f32,

                Y_PISTA,

                sz / 100.0,

                sz / 100.0,

                0.0,

                Color::new(1.0, 1.0, 1.0, alpha),

            );

        }



        for bp in &sess.balloon_pops {

            let sz = 40.0 + bp.age_frames * 12.0;

            let alpha = (1.0 - bp.age_frames * 0.08).max(0.0);

            draw_sprite_ex(

                assets.balloon_pop_fx.as_ref(),

                X_JUEZ as f32,

                Y_PISTA,

                sz / 100.0,

                sz / 100.0,

                0.0,

                Color::new(1.0, 1.0, 1.0, alpha),

            );

        }



        for b in &sess.judge_bursts {

            let f = b.age_frames;

            let sz = if f < 5.0 {

                f * 17.5

            } else if f < 8.0 {

                87.5 + (f - 5.0) * 2.5

            } else {

                95.0 + (f - 8.0) * 1.0

            };

            let alpha = if f < 8.0 { 1.0 } else { (1.0 - (f - 8.0) * 0.2).max(0.0) };

            let sp = match b.kind {

                JudgeKind::PGood => assets.burst_pgood.as_ref(),

                JudgeKind::Good => assets.burst_good.as_ref(),

                JudgeKind::Ok => assets.burst_ok.as_ref(),

                JudgeKind::Bad => None,

            };

            draw_sprite_ex(

                sp,

                X_JUEZ as f32,

                Y_PISTA,

                sz / 100.0,

                sz / 100.0,

                0.0,

                Color::new(1.0, 1.0, 1.0, alpha),

            );

        }



        gl_use_default_material();

    }



    // Juicios con Tilt (Sprite3)

    for j in &sess.judge_popups {

        let f = j.age_frames;

        let y_off = if f < 2.0 {

            5.0 + f * 17.5

        } else if f < 3.0 {

            40.0 - 5.0

        } else if f < 4.0 {

            35.0 - 2.5

        } else {

            32.5 - 1.25

        };

        let alpha = if f < 2.0 {

            0.2 + f * 0.4

        } else if f < 14.0 {

            1.0

        } else {

            (1.0 - (f - 14.0) * 0.2).max(0.0)

        };

        let sp = match j.kind {

            JudgeKind::PGood => assets.judge_pgood.as_ref(),

            JudgeKind::Good => assets.judge_good.as_ref(),

            JudgeKind::Ok => assets.judge_ok.as_ref(),

            JudgeKind::Bad => assets.judge_bad.as_ref(),

        };

        draw_sprite_ex(

            sp,

            X_JUEZ as f32,

            Y_PISTA + y_off,

            1.0,

            1.0,

            j.tilt_deg,

            Color::new(1.0, 1.0, 1.0, alpha),

        );

    }



    // 12. Icono 魂 (Sprite22) se dibuja ANTES de las notas voladoras y explosiones

    let soul_icon = if sess.tamashii >= 8000.0 {

        assets.soul_icon_on.as_ref()

    } else {

        assets.soul_icon_off.as_ref()

    };

    draw_sprite(soul_icon, SOUL_X, SOUL_Y, 1.0, WHITE);



    // Lote aditivo 3: estela de notas grandes (F3..F27)

    if !sess.big_glows.is_empty() {

        mats.additive_fx.set_uniform("u_hue_shift", 0.0_f32);

        mats.additive_fx.set_uniform("u_brightness", 0.0_f32);

        gl_use_material(&mats.additive_fx);

        for bg in &sess.big_glows {

            let sz = 25.0 + bg.age_frames * 5.0;

            let alpha = (1.0 - bg.age_frames / 15.0).max(0.0);

            draw_sprite_ex(

                assets.big_glow.as_ref(),

                bg.x,

                bg.y,

                sz / 100.0,

                sz / 100.0,

                0.0,

                Color::new(1.0, 1.0, 1.0, alpha),

            );

        }

        gl_use_default_material();

    }



    // Explosión de 魂 (explosion_soul) en coordenada fija (SOUL_X, SOUL_Y)

    for ex in &sess.explosions {

        let sp = if ex.age_frames < 2.0 {

            assets.ex1.as_ref()

        } else {

            assets.ex2.as_ref()

        };

        let alpha = if ex.age_frames < 12.0 {

            1.0

        } else {

            (1.0 - (ex.age_frames - 12.0) / 5.0).max(0.0)

        };

        draw_sprite_fx(

            mats,

            false,

            ex.color_fx * 1.8,

            0.0,

            sp,

            ex.x,

            ex.y,

            ex.size / 100.0,

            ex.size / 100.0,

            ex.angle,

            Color::new(1.0, 1.0, 1.0, alpha),

        );

    }



    // Notas voladoras (FlyDon, FlyKa, FlyDON, FlyKA, FlyBalloon) POR DELANTE de 魂

    for fn_item in &sess.flying_notes {

        let sp = match fn_item.kind {

            NoteKind::Don => assets.don.as_ref(),

            NoteKind::Ka => assets.ka.as_ref(),

            NoteKind::BigDon | NoteKind::Balloon => assets.big_don.as_ref(),

            NoteKind::BigKa => assets.big_ka.as_ref(),

            _ => assets.don.as_ref(),

        };

        if !fn_item.exploded {

            let rad = (fn_item.anim - 70.0).to_radians();

            let x = rad.sin() * 240.0 + 82.0;

            let y = rad.cos() * 240.0 - 40.0;

            draw_sprite(sp, x, y, 1.0, WHITE);

        } else {

            let (brightness, alpha) = if fn_item.kind.is_balloon() {

                let b = (fn_item.fade_frames * 0.04).clamp(0.0, 1.0);

                let a = if fn_item.fade_frames < 25.0 {

                    1.0

                } else {

                    (1.0 - (fn_item.fade_frames - 25.0) * 0.2).max(0.0)

                };

                (b, a)

            } else {

                let b = (fn_item.fade_frames * 0.1).clamp(0.0, 1.0);

                let a = if fn_item.fade_frames < 10.0 {

                    1.0

                } else {

                    (1.0 - (fn_item.fade_frames - 10.0) * 0.1).max(0.0)

                };

                (b, a)

            };

            draw_sprite_fx(

                mats,

                false,

                0.0,

                brightness,

                sp,

                SOUL_X,

                SOUL_Y,

                1.0,

                1.0,

                0.0,

                Color::from_rgba(255, 242, 126, (alpha * 255.0) as u8),

            );

        }

    }



    // 13. Nubes bailarinas / Fantasmas de fallo agrupados por tono

    if !sess.dancers.is_empty() {

        for d in &sess.dancers {

            if d.is_miss {

                draw_sprite(

                    assets.ghost_miss.as_ref(),

                    d.x,

                    d.y,

                    d.size / 100.0,

                    WHITE,

                );

            }

        }

        mats.normal_fx.set_uniform("u_brightness", 0.0_f32);

        for &fx_bucket in &[50.0_f32, 100.0, 150.0, 200.0] {

            let mut has_any = false;

            for d in &sess.dancers {

                if !d.is_miss && (d.color_fx - fx_bucket).abs() < 1.0 {

                    if !has_any {

                        mats.normal_fx.set_uniform("u_hue_shift", fx_bucket * 1.8);

                        gl_use_material(&mats.normal_fx);

                        has_any = true;

                    }

                    draw_sprite_ex(

                        assets.fly_balloon.as_ref(),

                        d.x,

                        d.y,

                        d.size / 100.0,

                        d.size / 100.0,

                        0.0,

                        WHITE,

                    );

                }

            }

            if has_any {

                gl_use_default_material();

            }

        }

    }



    // 14. Combo (Sprite9 & Sprite10)

    if sess.combo >= 10 {

        let is_high = sess.combo >= sess.course.combo_color_threshold();

        let size_pct = if is_high { 160.0 } else { 110.0 };

        let tint = if is_high {

            Color::from_rgba(255, 131, 0, 255)

        } else {

            WHITE

        };

        let stretch_y = 1.0 + sess.combo_anim;

        draw_digits_row(

            &assets.digits,

            &sess.combo.to_string(),

            -235.0,

            30.0 + sess.combo_anim * 2.0,

            size_pct,

            stretch_y,

            tint,

        );

        draw_sprite(assets.combo_label_ja.as_ref(), -235.0, 10.0, 0.5, tint);

    }



    // 15. Puntuación y Popups ScoreAdd escalonados por id (Score & ScoreAdd)

    draw_digits_right_aligned(

        &assets.digits,

        &sess.score.to_string(),

        -195.0,

        70.0 + sess.score_anim * 1.0,

        75.0,

        1.0 + sess.score_anim,

        WHITE,

    );



    for sp in &sess.score_popups {

        draw_score_add_digits(&assets.digits, &sp.text, sp.age_frames);

    }



    // 16. Título y Letras (Titulo, Letra)

    draw_text_outlined(&tja.title, font, 300.0, 150.0, 16.0, WHITE, BLACK, 1.0);

    if !sess.lyric_actual.is_empty() {

        draw_text_outlined(

            &sess.lyric_actual,

            font,

            0.0,

            -150.0,

            15.0,

            WHITE,

            Color::from_rgba(0, 93, 255, 255),

            0.5,

        );

    }



    if settings.auto_mode {

        draw_text_outlined("AUTO", font, 285.0, 130.0, 12.0, YELLOW, BLACK, 1.0);

    }

}



fn draw_results_overlay(

    assets: &SvgAssets,

    sess: &PlaySession,

    font: Option<&Font>,

    settings: &GameSettings,

) {

    draw_rect_scratch(-320.0, 180.0, 640.0, 360.0, Color::new(0.0, 0.0, 0.0, 0.65));



    // Misses break full-combo crowns too, whether counted as Bad or kept separate.
    let no_bad_or_miss = sess.bad == 0 && sess.miss == 0;

    let crown_sp = if no_bad_or_miss && sess.ok == 0 && sess.good == 0 {

        assets.crown_pdc.as_ref()

    } else if no_bad_or_miss && sess.ok == 0 {

        assets.crown_dfc.as_ref()

    } else if no_bad_or_miss {

        assets.crown_fc.as_ref()

    } else if sess.tamashii >= 10000.0 {

        assets.crown_full_gauge.as_ref()

    } else if sess.tamashii >= 8000.0 {

        assets.crown_clear.as_ref()

    } else {

        assets.crown_fail.as_ref()

    };



    let f = sess.finish_anim_frames;

    if f > 10.0 {

        let t = (f - 10.0).min(25.0);

        let alpha = (t * 0.04).clamp(0.0, 1.0);

        let size_pct = 20.0 - t * 0.4;

        draw_sprite(

            crown_sp,

            -157.0,

            100.0,

            size_pct / 100.0,

            Color::new(1.0, 1.0, 1.0, alpha),

        );

    }



    let total = sess.total_notes.max(1) as f32;

    let bar_anim = (sess.finish_anim_frames / 50.0).clamp(0.0, 1.0);

    let mut bars: Vec<(&str, u32, Option<&SpriteTex>)> = vec![

        ("PGood", sess.pgood, assets.res_bar_pgood.as_ref()),

        ("Good", sess.good, assets.res_bar_good.as_ref()),

        ("Ok", sess.ok, assets.res_bar_ok.as_ref()),

        ("Bad", sess.bad, assets.res_bar_bad.as_ref()),

    ];

    if settings.separate_misses {

        bars.push(("Miss", sess.miss, assets.res_bar_bad.as_ref()));

    }

    let rows = bars.len() as f32;

    let top = (rows - 1.0) * 12.5;

    for (i, (label, count, bar_sp)) in bars.into_iter().enumerate() {

        if label == "PGood" && !settings.pgood_mode {

            continue;

        }

        let y = top - i as f32 * 25.0;

        draw_text_outlined(label, font, -140.0, y, 16.0, WHITE, BLACK, 1.0);

        let stretch_x = ((count as f32) / total).clamp(0.0, 1.0) * bar_anim;

        draw_sprite_ex(bar_sp, -120.0, y, stretch_x * 1.1, 1.6, 0.0, WHITE);

        draw_text_outlined(&count.to_string(), font, 115.0, y, 16.0, WHITE, BLACK, 0.0);

    }



    draw_text_outlined(

        &format!(

            "Score: {}   |   Max Combo: {}   |   Precisión: {:.2}% ({})",

            sess.score,

            sess.max_combo,

            sess.accuracy_percent(),

            sess.rank_label()

        ),

        font,

        0.0,

        -80.0,

        15.0,

        WHITE,

        BLACK,

        0.5,

    );

}

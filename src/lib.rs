// Android necesita un cdylib (lib<name>.so) con el entrypoint `android_main`.
// Todo el código real vive en main.rs; lo incluimos en la lib para no duplicarlo.
include!("main.rs");

/// Entry point que llama la `MainActivity` de miniquad vía JNI.
/// En Android `main()` (generado por `#[macroquad::main]`) inicia el bucle
/// de miniquad y retorna; el render corre en su propio hilo.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn quad_main() {
    main();
}

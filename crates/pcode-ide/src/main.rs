//! Binario `pcode-ide`: `pcode-ide --url http://127.0.0.1:PUERTO/?t=TOKEN`.
use pcode_ide::{app::Ide, rpc};

fn main() {
    let url = std::env::args()
        .skip_while(|a| a != "--url")
        .nth(1)
        .unwrap_or_default();
    let ep = match rpc::Endpoint::parse(&url) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("uso: pcode-ide --url http://127.0.0.1:PUERTO/?t=TOKEN ({e})");
            std::process::exit(2);
        }
    };
    // Lanzado por `pcode ide`: cuando el demonio muere se cierra su tubo y el IDE también acaba.
    if std::env::args().any(|a| a == "--hasta-eof") {
        std::thread::spawn(|| {
            use std::io::Read;
            let mut buf = [0u8; 64];
            let mut stdin = std::io::stdin();
            while matches!(stdin.read(&mut buf), Ok(n) if n > 0) {}
            std::process::exit(0);
        });
    }
    let lang = std::env::var("LANG").unwrap_or_default();
    let t = pcode_ide::texts::for_locale(&lang);
    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 700.0])
            .with_title(t.window_title),
        ..Default::default()
    };
    let r = eframe::run_native(
        t.window_title,
        opts,
        Box::new(move |cc| Ok(Box::new(Ide::new(rpc::Client::new(ep), &cc.egui_ctx, t)))),
    );
    if let Err(e) = r {
        // `pcode ide` reconoce este código y reintenta con render por software.
        eprintln!("No se pudo abrir la ventana con OpenGL: {e}");
        std::process::exit(3);
    }
}

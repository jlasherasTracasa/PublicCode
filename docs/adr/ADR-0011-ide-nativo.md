# ADR-0011: IDE nativo con egui sobre OpenGL

**Estado**: aceptada

## Contexto
Los equipos modestos no aguantan un navegador además del editor. Hace falta una aplicación de escritorio sin navegador, sin mover lógica fuera del núcleo.

## Decisión
- Crate `pcode-ide` con `eframe` 0.37 **sin wgpu** (`glow`, `x11`, `clipboard`, `default_fonts`).
- Es un cliente del demonio: JSON-RPC y SSE por loopback. Cero lógica de negocio.
- Se repinta solo con eventos.
- Wayland nativo y accesibilidad (AccessKit) quedan como features opcionales (F9.22).

## Alternativas
| Opción | Por qué no |
|---|---|
| Tauri (WebView) | ya hay un navegador por debajo; más RAM |
| wgpu | compila mucho más y arrastra más dependencias |
| Slint / iced | menos widgets de texto maduros |

## Consecuencias
`eframe` es una dependencia nueva (AGENTS.md §7 la incluye al listar las de interfaz).

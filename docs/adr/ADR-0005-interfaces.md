# ADR-0005: interfaces centradas en el demonio, escritorio con Tauri 2 y UI común en React

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

PublicCode debe usarse desde la terminal (CLI y TUI), como aplicación de escritorio en Linux y
Windows, desde el navegador (servidores sin escritorio), desde el móvil (aprobaciones), desde el IDE
y desde GitHub. Mantener lógica en cada cliente multiplicaría los errores; mantener una UI distinta
para web y para escritorio duplicaría el trabajo. Además, las tareas las implementa sobre todo
Haiku 5.5, que rinde mejor con tecnologías muy extendidas.

## Decisión

1. **Demonio único** `pcoded` con toda la lógica y una API definida en `crates/pcode-protocol`
   (JSON-RPC 2.0 más eventos). Los tipos se exportan a TypeScript con `ts-rs`.
2. **UI común** `apps/ui` en React + TypeScript estricto + Vite, con un `Transport` abstracto
   (`http` en el navegador y en la PWA; `tauri` en el escritorio).
3. **Escritorio** con **Tauri 2**: carga `apps/ui`, lanza `pcoded` como sidecar y aporta la
   integración con el sistema (bandeja, notificaciones, enlaces, llavero, actualizaciones firmadas).
4. **TUI** nativa en Rust con `ratatui`, cliente del mismo demonio.
5. **IDE** por ACP y MCP; la extensión de VS Code es un webview de `apps/ui`.

## Alternativas

- **Electron**: maduro, pero pesado (≥ 150 MB por instalación y mucha RAM) y otro runtime que mantener.
- **UI nativa en Rust (egui, iced, Slint)**: un solo lenguaje, pero menos ecosistema (diffs, Markdown,
  accesibilidad) y menos material de entrenamiento para Haiku; además no sirve para la web.
- **Una UI distinta para web y escritorio**: el doble de trabajo y de errores.
- **Que la TUI también fuera web**: la terminal pide su propio lenguaje de interacción; además, la
  TUI debe funcionar por SSH sin navegador.

## Consecuencias

+ Una sola UI para web, escritorio, PWA y VS Code; la TUI y la CLI comparten el cliente Rust.
+ Instaladores pequeños (Tauri usa el webview del sistema) y la seguridad de Tauri (capacidades por
  comando y CSP).
+ Las sesiones se mueven entre clientes porque el estado vive en el demonio.
− Dos pilas (Rust y TypeScript): AGENTS.md fija convenciones y comprobaciones para cada una.
− Las diferencias entre los webviews del sistema (WebKitGTK y WebView2) requieren pruebas
  en Linux y Windows: lo hace la CI de empaquetado (F5.21).

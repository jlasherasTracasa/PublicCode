---
name: interfaces
description: Cómo implementar y probar tareas de interfaz de PublicCode (TUI con ratatui, UI común React/TypeScript, escritorio Tauri 2, web/PWA, ACP, VS Code) sin meter lógica en el cliente. Úsala en cualquier tarea F5.x o que toque crates/pcode-tui, apps/ui, apps/desktop o apps/vscode.
---

# Tareas de interfaz

Diseño: `docs/interfaces.md` y `docs/adr/ADR-0005-interfaces.md`. Reglas: `AGENTS.md`, secciones 6.2 a 6.5.

## Regla de oro

El cliente **no decide nada**: muestra el estado del demonio y envía peticiones JSON-RPC. Si
necesitas calcular algo de negocio (qué tarea va antes, si una acción está permitida), falta un
método en la API: escríbelo en *Pendiente* en lugar de implementarlo en el cliente.

## Tipos de la API

- Vienen de `crates/pcode-protocol`; en TypeScript, de `apps/ui/src/protocol/` (generados, **no editar**).
- Si falta un campo, la tarea correcta es cambiar `pcode-protocol` y regenerar (`cargo test -p pcode-protocol`), no definir un tipo local.

## TUI (ratatui)

- Estado en `App`; `update(Event) -> Vec<Command>` sin E/S; `render(&App, &mut Frame)` sin efectos.
- Test de render:

```rust
let backend = ratatui::backend::TestBackend::new(80, 24);
let mut term = ratatui::Terminal::new(backend).unwrap();
term.draw(|f| render(&app, f)).unwrap();
let texto = term.backend().buffer().content().iter().map(|c| c.symbol()).collect::<String>();
assert!(texto.contains("Sesiones"));
```

- Prueba siempre a 80×24 y a 120×40. Símbolos además de colores (`✓ ✗ ⏳ ⚠ ⧗`). Textos con `t(lang, clave)`.

## UI común (apps/ui)

- Componentes de función con TS estricto; estado con `zustand`; textos con i18n (`es` y `en`).
- **Nunca** `fetch` ni `@tauri-apps/api` en un componente: usa `useTransport()`.
- Tests con el transporte falso:

```tsx
const t = createMockTransport({ "session.list": () => [] });
render(<TransportContext.Provider value={t}><Sesion /></TransportContext.Provider>);
t.emit({ tipo: "Transcript", session: 1, entry: { tipo: "User", text: "hola" } });
expect(await screen.findByText("hola")).toBeInTheDocument();
```

- Busca por rol y texto (`getByRole("button", { name: … })`), no por clases. Comprueba el teclado con `user-event`.
- El Markdown de los agentes, sin HTML crudo; diffs y salidas como texto.

## Escritorio (Tauri 2)

- Cada acción nativa es un `#[tauri::command]` concreto; nada de `shell` ni `fs` genérico en `capabilities`.
- La lógica (qué notificar, estado de la bandeja, enlaces) va en funciones puras con tests; lo nativo, en llamadas finas.
- No ejecutes `tauri build` (lo hace la CI de empaquetado): `cargo clippy -p pcode-desktop` y `cargo test -p pcode-desktop`.
- Credenciales solo con `SecretStore` (llavero); en los tests, `MemoryStore`.

## Accesibilidad y rendimiento (comprueba antes de terminar)

- [ ] Todo se puede hacer con teclado; el foco se ve.
- [ ] Los colores llevan un símbolo o texto que los acompaña.
- [ ] Ningún literal visible fuera del catálogo i18n.
- [ ] La TUI no bloquea el render; la UI no carga nada remoto.

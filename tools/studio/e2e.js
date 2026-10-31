#!/usr/bin/env node
// Prueba de extremo a extremo de PublicCode Studio con un navegador real (Playwright + Chromium).
//
// Crea un proyecto de ejemplo (git + Rust + Markdown) en un directorio temporal, arranca
// `pcode studio` con un modelo simulado (PCODE_MOCK_SCRIPT, sin red) y recorre la interfaz:
// explorador, editor, migas de pan, esquema, símbolos, marcas de git, temas, paneles, git con
// ✦ Generar y ✦ Revisar, el agente con su tarjeta de cambios, deshacer/rehacer, actividad,
// terminal con ✦ Arreglar, ✦ Concretar, paleta y móvil. Termina con código 1 si algo falla.
//
// Uso:
//   cargo build -p pcode-cli
//   node tools/studio/e2e.js [--pcode target/debug/pcode] [--capturas docs/img]
//
// Necesita `playwright` con Chromium (npm i -g playwright && npx playwright install chromium).
// No es una dependencia del proyecto: la CI no lo ejecuta; sirve para comprobar Studio a mano y
// para regenerar las capturas del README.
'use strict';
const { chromium } = require('playwright');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { execFileSync, spawn } = require('child_process');

const arg = (n, def) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : def; };
const RAIZ = path.resolve(__dirname, '..', '..');
const PCODE = path.resolve(arg('--pcode', path.join(RAIZ, 'target', 'debug', process.platform === 'win32' ? 'pcode.exe' : 'pcode')));
const CAPTURAS = arg('--capturas', null);
const TMP = fs.mkdtempSync(path.join(os.tmpdir(), 'pcode-e2e-'));
const D = path.join(TMP, 'demo');
const MOCK = path.join(TMP, 'mock.json');
const enPath = (b) => (process.env.PATH || '').split(path.delimiter).some((d) => fs.existsSync(path.join(d, b)));
const PYRIGHT = enPath('pyright-langserver');

function resp(content, calls) {
  return { message: { role: 'assistant', content, tool_calls: calls || [] }, usage: { input_tokens: 120, output_tokens: 40, cached_input_tokens: 0 }, stop_reason: calls && calls.length ? 'ToolUse' : 'EndTurn' };
}
const mock = (...r) => fs.writeFileSync(MOCK, JSON.stringify(r));
const git = (...a) => execFileSync('git', ['-C', D, ...a]).toString();

function proyecto() {
  fs.mkdirSync(path.join(D, 'src'), { recursive: true });
  fs.writeFileSync(path.join(D, 'Cargo.toml'), '[package]\nname = "demo"\nversion = "0.1.0"\nedition = "2024"\n');
  fs.writeFileSync(path.join(D, 'src', 'lib.rs'), '//! Utilidades de la demo.\n\n/// Devuelve el doble de `x`.\npub fn doble(x: i32) -> i32 {\n    x * 2\n}\n\n/// Una caché muy simple.\npub struct Cache {\n    datos: Vec<i32>,\n}\n\nimpl Cache {\n    pub fn nueva() -> Self {\n        Cache { datos: vec![] }\n    }\n\n    pub fn guarda(&mut self, v: i32) {\n        self.datos.push(v);\n    }\n}\n');
  fs.writeFileSync(path.join(D, 'src', 'main.rs'), 'use demo::{doble, Cache};\n\nfn main() {\n    let mut cache = Cache::nueva();\n    cache.guarda(doble(21));\n    // TODO: mostrar la caché\n    println!("listo");\n}\n');
  fs.writeFileSync(path.join(D, 'README.md'), '# Demo\n\n## Uso\n\n`cargo run`\n\n## Tests\n\n`cargo test`\n');
  if (PYRIGHT) {
    fs.mkdirSync(path.join(D, 'app'), { recursive: true });
    fs.writeFileSync(path.join(D, 'app', 'util.py'), 'def doble(x: int) -> int:\n    return x * 2\n');
    fs.writeFileSync(path.join(D, 'app', 'main.py'), 'from util import doble\n\n\ndef principal() -> str:\n    return doble(21)\n');
  }
  git('init', '-q'); git('config', 'user.email', 'demo@ejemplo.org'); git('config', 'user.name', 'Demo');
  git('add', '.'); git('commit', '-qm', 'feat: proyecto de ejemplo');
}

function arranca() {
  return new Promise((ok, mal) => {
    const p = spawn(PCODE, ['studio', '--no-abrir', '--puerto', '0'], { cwd: D, env: { ...process.env, PCODE_MOCK_SCRIPT: MOCK, NO_COLOR: '1' } });
    let err = '';
    const t = setTimeout(() => mal(new Error('Studio no arrancó en 20 s: ' + err)), 20000);
    p.stderr.on('data', (b) => { err += b; const m = err.match(/http:\/\/\S+/); if (m) { clearTimeout(t); ok({ proc: p, url: m[0] }); } });
    p.on('exit', (c) => mal(new Error('Studio terminó con ' + c + ': ' + err)));
  });
}

(async () => {
  if (!fs.existsSync(PCODE)) { console.error('No existe ' + PCODE + ': ejecuta antes `cargo build -p pcode-cli`.'); process.exit(2); }
  proyecto();
  mock(resp('Voy a añadir un test para `doble`.', [{ id: '1', name: 'escribir', arguments: { path: 'tests/doble.rs', content: '#[test]\nfn doble_funciona() {\n    assert_eq!(demo::doble(21), 42);\n}\n' } }]),
    resp('Listo: he creado `tests/doble.rs` con un test para **doble**. Ejecútalo con `cargo test`.'));
  const { proc, url } = await arranca();
  const navegador = await chromium.launch();
  const errores = [];
  let fallos = 0;
  const ok = (c, msg) => { console.log((c ? 'OK    ' : 'FALLO ') + msg); if (!c) fallos++; };
  const captura = async (pg, nombre) => { if (CAPTURAS) await pg.screenshot({ path: path.join(CAPTURAS, nombre) }); };
  const pagina = async (opts) => {
    const ctx = await navegador.newContext({ viewport: { width: 1440, height: 900 }, colorScheme: 'light', ...opts });
    const pg = await ctx.newPage();
    pg.on('pageerror', (e) => errores.push('pageerror: ' + e.message));
    pg.on('console', (m) => { if (m.type() === 'error' && !m.text().includes('favicon')) errores.push('console: ' + m.text()); });
    return pg;
  };
  try {
    const p = await pagina();
    await p.goto(url);
    await p.waitForSelector('.nodo');
    ok(await p.locator('.card.asistente .accion').count() === 4, 'inicio: tarjeta del asistente con 4 acciones');
    await captura(p, 'studio-inicio.png');

    // ── Editor, migas de pan, esquema y símbolos ──
    await p.locator('.nodo', { hasText: 'src' }).first().click();
    await p.locator('.nodo', { hasText: 'lib.rs' }).first().click();
    await p.waitForSelector('#editor:not(.oculto)');
    ok(await p.locator('#hl-code .tk-kw').count() > 3, 'resaltado de sintaxis');
    ok((await p.textContent('#e-lang')) === 'Rust', 'barra de estado: Rust');
    await p.keyboard.press('Control+g'); // ir a línea con diálogo
    await p.waitForTimeout(100);
    await p.keyboard.press('Escape');
    await p.click('#ta');
    await p.evaluate(() => { const t = document.getElementById('ta'); const pos = t.value.indexOf('self.datos.push'); t.setSelectionRange(pos, pos); t.dispatchEvent(new Event('keyup')); });
    await p.waitForTimeout(300);
    const migas = await p.textContent('#migas');
    ok(migas.includes('src') && migas.includes('lib.rs') && migas.includes('Cache') && migas.includes('guarda'), 'migas de pan con el símbolo del cursor: ' + migas.replace(/\s+/g, ' '));
    await p.click('.actividad button[data-panel="esquema"]');
    await p.waitForTimeout(200);
    const esquema = await p.locator('#esquema .simbolo').allTextContents();
    ok(esquema.length === 5 && esquema.some((t) => t.includes('doble')), 'esquema: ' + esquema.length + ' símbolos');
    ok(await p.locator('#esquema .simbolo.actual', { hasText: 'guarda' }).count() === 1, 'esquema: marca el símbolo actual');
    await p.fill('#q-esquema', 'nue'); await p.waitForTimeout(100);
    ok(await p.locator('#esquema .simbolo').count() === 1, 'esquema: filtro');
    await p.fill('#q-esquema', '');
    await p.keyboard.press('Control+t'); await p.keyboard.type('doble'); await p.waitForTimeout(500);
    ok((await p.locator('#l-archivos li').first().textContent()).includes('doble'), 'Ctrl+T encuentra el símbolo doble');
    await p.keyboard.press('Enter'); await p.waitForTimeout(300);
    ok((await p.textContent('#e-pos')).startsWith('Lín 4'), 'Ctrl+T salta a la línea del símbolo');

    // ── Marcas de git ──
    await p.click('.actividad button[data-panel="archivos"]');
    await p.locator('.nodo', { hasText: 'main.rs' }).first().click(); await p.waitForTimeout(300);
    await p.click('#ta'); await p.keyboard.press('Control+End'); await p.keyboard.type('\n// editado desde Studio');
    await p.keyboard.press('Control+s'); await p.waitForTimeout(600);
    ok(fs.readFileSync(path.join(D, 'src', 'main.rs'), 'utf8').includes('// editado desde Studio'), 'Ctrl+S guarda');
    ok(await p.locator('#capa-git .gm.added').count() >= 1, 'marca de git en el margen (línea nueva)');
    ok(await p.locator('.aviso.ok', { hasText: 'Guardado' }).count() === 1, 'aviso «Guardado»');

    // ── Temas y paneles ──
    const tema0 = await p.evaluate(() => document.documentElement.dataset.theme);
    await p.click('#b-tema'); await p.waitForTimeout(100);
    const tema1 = await p.evaluate(() => document.documentElement.dataset.theme);
    ok(tema0 === 'claro' && tema1 === 'claro', 'tema automático → claro (sistema claro)');
    await p.click('#b-tema'); await p.click('#b-tema'); await p.waitForTimeout(100);
    ok(await p.evaluate(() => document.documentElement.dataset.theme === 'medianoche' && 'dark' in document.documentElement.dataset), 'tema medianoche');
    await captura(p, 'studio-medianoche.png');
    await p.click('#b-tema'); await p.waitForTimeout(100);
    const asa = await p.locator('.lateral .asa').boundingBox();
    await p.mouse.move(asa.x + 3, asa.y + 200); await p.mouse.down(); await p.mouse.move(asa.x + 83, asa.y + 200, { steps: 4 }); await p.mouse.up();
    const ancho = await p.evaluate(() => document.querySelector('.lateral').getBoundingClientRect().width);
    ok(Math.abs(ancho - 360) < 6, 'panel lateral redimensionado a ~360 px: ' + Math.round(ancho));

    // ── Git: generar mensaje y revisar ──
    mock(resp('docs: comenta el final de main\n\n- Añade un comentario al final de `main`.'));
    await p.click('.actividad button[data-panel="git"]'); await p.waitForTimeout(400);
    await p.click('#b-generar'); await p.waitForFunction(() => document.getElementById('msg-commit').value.length > 0, null, { timeout: 15000 });
    ok((await p.inputValue('#msg-commit')).startsWith('docs: comenta el final de main'), '✦ Generar mensaje de commit');
    mock(resp('## Veredicto\n\nListo para commit.\n\n## Detalles\n\n- 🔵 `src/main.rs:9`: el comentario no aporta; quítalo o explica qué falta.\n- Los tests no cambian.'));
    await p.click('#b-revisar');
    await p.waitForSelector('#documento .md h2', { timeout: 15000 });
    ok(await p.locator('#documento .md li').count() === 2, '✦ Revisar: documento con la revisión en Markdown');
    ok(await p.locator('.pestana', { hasText: 'Revisión de los cambios' }).count() === 1, 'pestaña de la revisión');
    await captura(p, 'studio-revision.png');

    // ── Agente: tarjeta de cambios, deshacer y rehacer ──
    mock(resp('Voy a añadir un test para `doble`.', [{ id: '1', name: 'escribir', arguments: { path: 'tests/doble.rs', content: '#[test]\nfn doble_funciona() {\n    assert_eq!(demo::doble(21), 42);\n}\n' } }]),
      resp('Listo: he creado `tests/doble.rs` con un test para **doble**. Ejecútalo con `cargo test`.'));
    await p.fill('#mensaje', 'Añade un test para doble'); await p.click('#b-enviar');
    await p.waitForSelector('.tarjeta-turno', { timeout: 15000 });
    ok(await p.locator('.burbuja.assistant', { hasText: 'Listo' }).count() === 1, 'el agente responde');
    ok((await p.textContent('.tarjeta-turno')).includes('1 archivo cambiado'), 'tarjeta de fin de turno');
    ok(fs.existsSync(path.join(D, 'tests', 'doble.rs')), 'el agente creó tests/doble.rs');
    ok((await p.textContent('#e-agente')).includes('#'), 'barra de estado: sesión del agente');
    await p.click('.actividad button[data-panel="archivos"]'); await p.waitForTimeout(500);
    await p.locator('.nodo', { hasText: 'lib.rs' }).first().click(); await p.waitForTimeout(300);
    await captura(p, 'studio-claro.png');
    await p.locator('.tarjeta-turno button', { hasText: 'Deshacer' }).click(); await p.waitForTimeout(600);
    ok(!fs.existsSync(path.join(D, 'tests', 'doble.rs')), '↶ Deshacer el turno borra lo que creó');
    ok(await p.locator('.tarjeta-turno.deshecho').count() === 1, 'la tarjeta queda como deshecha');
    await p.click('.actividad button[data-panel="actividad"]'); await p.waitForTimeout(500);
    ok((await p.textContent('#actividad')).includes('Añade un test para doble'), 'actividad: el turno con su petición');
    ok(await p.locator('.integridad:not(.rota)').count() === 1, 'actividad: auditoría íntegra');
    ok(await p.locator('.evento .a', { hasText: 'escribir' }).count() >= 1, 'actividad: la acción escribir');
    await p.locator('#actividad button', { hasText: 'Rehacer' }).first().click(); await p.waitForTimeout(600);
    ok(fs.existsSync(path.join(D, 'tests', 'doble.rs')), '↷ Rehacer lo vuelve a aplicar');
    await captura(p, 'studio-actividad.png');

    // ── Terminal con ✦ Arreglar ──
    await p.keyboard.press('Control+Backquote'); await p.waitForTimeout(200);
    await p.fill('#orden', 'ls no-existe-este-archivo'); await p.press('#orden', 'Enter');
    await p.waitForSelector('.fallo-terminal', { timeout: 10000 });
    ok(await p.locator('.fallo-terminal .chispa').count() === 1, 'terminal: ✦ Arreglar con el agente tras un fallo');
    await p.click('#t-cerrar');

    // ── Concretar ──
    mock(resp('**Objetivo:** que `Cache` no crezca sin límite.\n**Dónde:** src/lib.rs (struct Cache, fn guarda).\n**Pasos:** 1. añadir capacidad máxima. 2. descartar el más antiguo.\n**Hecho cuando:** `cargo test` pasa con un test de capacidad.\n**Preguntas:** ¿qué capacidad por defecto?'));
    await p.selectOption('#sesion', ''); await p.waitForTimeout(200);
    await p.fill('#mensaje', 'que la cache no crezca');
    await p.click('#b-concretar');
    await p.waitForFunction(() => document.getElementById('mensaje').value.startsWith('**Objetivo:**'), null, { timeout: 15000 });
    ok((await p.inputValue('#mensaje')).includes('**Preguntas:**'), '✦ Concretar reescribe la petición');
    await p.fill('#mensaje', '');

    // ── Paleta ──
    await p.keyboard.press('Control+Shift+p'); await p.keyboard.type('medianoche'); await p.waitForTimeout(150);
    ok((await p.locator('#l-archivos li').first().textContent()).includes('Tema: medianoche'), 'paleta: Tema medianoche');
    await p.keyboard.press('Escape');

    // ── LSP (solo si pyright está instalado): problemas y hover con ✦ Arreglar ──
    if (PYRIGHT) {
      await p.click('.actividad button[data-panel="archivos"]');
      await p.locator('.nodo', { hasText: 'app' }).first().click();
      await p.locator('.nodo', { hasText: 'main.py' }).first().click();
      await p.waitForSelector('.subr.s1', { timeout: 40000 }).catch(() => {});
      ok(await p.locator('.subr.s1').count() >= 1, 'LSP: el error de tipo aparece subrayado');
      await p.keyboard.press('Control+Shift+M'); await p.waitForTimeout(300);
      ok(await p.locator('.problema .arreglar').count() >= 1 && await p.locator('.grupo-titulo .chispa', { hasText: 'Arreglar todos' }).count() === 1, 'Problemas: ✦ por problema y «Arreglar todos»');
      const caja = await p.locator('#capa-diag .subr.s1').first().boundingBox();
      await p.mouse.move(caja.x + 6, caja.y - 6); await p.waitForSelector('#hover:not(.oculto) .chispa', { timeout: 8000 }).catch(() => {});
      ok(await p.locator('#hover .chispa', { hasText: 'Arreglar' }).count() === 1, 'hover del error con ✦ Arreglar con el agente');
      await captura(p, 'studio-lsp.png');
      await p.locator('#hover .chispa').click(); await p.waitForTimeout(800);
      const pedido = (await p.locator('.burbuja.user').allTextContents()).join(' || ');
      ok(pedido.includes('Arregla este problema de app/main.py') && pedido.includes('Dentro de: fn principal') && pedido.includes('▶    5 │'), 'el agente recibe el diagnóstico estructurado (símbolo y código)');
    } else console.log('(sin pyright-langserver: se saltan las comprobaciones de LSP)');

    // ── Persistencia del diseño ──
    const p2 = await p.context().newPage();
    p2.on('pageerror', (e) => errores.push('pageerror: ' + e.message));
    await p2.goto(url); await p2.waitForSelector('.nodo'); await p2.waitForTimeout(300);
    const ancho2 = await p2.evaluate(() => document.querySelector('.lateral').getBoundingClientRect().width);
    ok(Math.abs(ancho2 - 360) < 6, 'el ancho del panel se recuerda al recargar');
    await p2.close();

    // ── Oscuro y móvil ──
    const d = await pagina({ colorScheme: 'dark' });
    await d.goto(url); await d.waitForSelector('.nodo');
    ok(await d.evaluate(() => document.documentElement.dataset.theme === 'oscuro'), 'tema automático → oscuro (sistema oscuro)');
    await d.locator('.nodo', { hasText: 'src' }).first().click(); await d.locator('.nodo', { hasText: 'lib.rs' }).first().click();
    await d.click('.actividad button[data-panel="esquema"]');
    await d.selectOption('#sesion', { index: 1 }); await d.waitForTimeout(800);
    await captura(d, 'studio-oscuro.png');
    const m = await pagina({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
    await m.goto(url); await m.waitForTimeout(800);
    await m.selectOption('#sesion', { index: 1 }); await m.waitForTimeout(500);
    ok(await m.locator('.nav-movil').isVisible(), 'móvil: navegación inferior');
    await captura(m, 'studio-movil.png');
  } catch (e) {
    ok(false, 'excepción: ' + (e.stack || e));
  } finally {
    ok(errores.length === 0, 'sin errores de JavaScript ' + JSON.stringify(errores));
    await navegador.close();
    proc.kill();
    fs.rmSync(TMP, { recursive: true, force: true });
  }
  console.log(fallos ? `\n${fallos} comprobaciones fallaron` : '\nTodo bien');
  process.exit(fallos ? 1 : 0);
})();

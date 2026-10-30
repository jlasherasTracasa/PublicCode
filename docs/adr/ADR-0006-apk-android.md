# ADR-0006: APK de Android como cliente fino (WebView) del demonio

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

La PWA (ADR-0005) cubre el móvil, pero exige servir la web por HTTPS para instalarse, y el demonio
aún no tiene TLS. Hace falta ya un APK instalable para seguir sesiones y resolver aprobaciones desde
Android, sin duplicar lógica (regla de arquitectura de AGENTS.md).

## Decisión

1. `apps/android`: app en **Java** con un único `Activity` y un `WebView` que carga la web de
   `pcoded`. Sin AndroidX ni otras bibliotecas; solo el plugin de Android para Gradle y, en tests,
   JUnit 4.
2. Emparejamiento: el usuario pega la URL que imprime `pcode daemon --escuchar 0.0.0.0`
   (`http://IP:7878/?t=…`). La app guarda solo el origen; el token se cambia en la primera carga por
   la cookie HttpOnly del demonio. `PCODE_DAEMON_TOKEN` fija el token entre reinicios.
3. Navegación limitada al origen del demonio; cualquier otro enlace se abre fuera. Sin acceso a
   archivos ni puente JavaScript.
4. HTTP en claro permitido (red local o túnel WireGuard/Tailscale) hasta que el demonio sirva TLS.
5. La CI compila, prueba y pasa lint; la release publica `pcode-<versión>-android.apk`, firmado con
   la clave de los secretos `PCODE_ANDROID_*` o, si no hay, con la de depuración.

## Alternativas

- **Tauri 2 para Android**: reutilizaría `apps/ui`, pero esa UI aún no existe (F5.11+) y añade NDK y
  Rust para Android a la CI. Se revisará cuando exista `apps/desktop`.
- **Solo PWA**: necesita TLS y no da un APK.
- **Kotlin + AndroidX**: más idiomático, pero más dependencias para una sola pantalla.

## Consecuencias

+ APK de unos 15 KB, sin lógica propia: todo lo nuevo de la web del demonio llega gratis.
− Tráfico sin cifrar fuera de un túnel: aviso en la CLI y en la documentación.
− Sin notificaciones push todavía (necesitan un servicio en segundo plano; F5.24).

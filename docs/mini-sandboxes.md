# Mini sandboxes

Un agente necesita poder romper cosas sin romper nada: probar dos enfoques a la vez, ejecutar código
que no es suyo, equivocarse con `git` y deshacerlo. Hoy SO_IA lo resuelve con un contenedor Docker y
un `git clone` por tarea, y aun así los agentes comparten el árbol en el modo `kernel`. PublicCode
usa el aislamiento **más ligero que baste**.

## Niveles

| Nivel | Mecanismo | Arranque | Aislamiento | Uso |
|---|---|---|---|---|
| **N0** | WASM (wasmtime) sin capacidades WASI salvo las concedidas | µs | memoria y E/S explícitas | tools puras |
| **N1** | namespaces de usuario, mount, PID y red + overlay CoW + raíz ro + Landlock + seccomp + límites | ~55 ms (prototipo Python; en Rust se espera < 10 ms) | archivos, red y procesos; mismo kernel | cada orden y cada subagente |
| **N2** | contenedor OCI (Docker/Podman) con la imagen del proyecto | 0,5–2 s | más N1, imagen propia | toolchains completos, QEMU/KVM |
| **N3** | microVM (Firecracker / Cloud Hypervisor) | 0,1–1 s | kernel propio | código no confiable, red abierta |

## N1 en detalle

### Archivos: overlay copy-on-write

```
 capa inferior (solo lectura): el repo real      ← compartida por todas las sandboxes
 capa superior: <estado>/upper                    ← una por sandbox
 vista: el repo montado en la misma ruta          ← lo que ve el agente
```

* El agente trabaja en la ruta de siempre (los builds no se enteran de nada).
* **Paralelismo sin clones**: N sandboxes comparten la capa inferior y cada una tiene su capa
  superior.
* `cambios`: recorre la capa superior; los dispositivos de caracteres 0/0 son borrados y los
  directorios opacos son reemplazos; filtra lo que ignora git.
* `aplicar`: copia la capa al repo. En F3, «aplicar» pasa por la cola de integración (rebase,
  lockfiles, re-validación).
* `descartar`: borra la capa, y eso es todo el «deshacer».

### Resto del sistema

* Todos los montajes existentes se remontan en **solo lectura**, salvo las rutas `--escribible`
  (bind sobre sí mismas antes del remontaje), por ejemplo `~/.cargo/registry`.
* `/tmp` y `HOME` nuevos y vacíos (tmpfs, o dentro del estado si el repo vive bajo `/tmp`).
* PID namespace con `--kill-child`: al terminar o vencer el tiempo, no queda ningún proceso.
* En Rust (F2.04 y F2.05): **Landlock** para restringir el acceso a archivos también por la API del
  kernel, y **seccomp** para bloquear `ptrace`, `mount` (después de la preparación), `kexec`,
  `bpf`, `keyctl`, etc.
* Límites: tiempo de pared; memoria y CPU con cgroups v2 si hay delegación (`systemd-run --user
  --scope`) y, si no, `RLIMIT_AS` y `RLIMIT_NPROC`.

### Red

Por defecto, **sin red** (namespace de red vacío, solo loopback). Los agentes externos sí necesitan
llegar a su API, así que hay un modo **red filtrada** (F2.19 y F2.20):

```
 sandbox (netns sin interfaces)                       host
 ┌───────────────────────────────┐            ┌──────────────────────────────────┐
 │ agente ──HTTPS_PROXY──► 127.0.0.1:3128      │ pcode-netproxy                   │
 │                 puente TCP→unix ──────────────► CONNECT host:443               │
 │                 (socket montado)│           │  ├─ ¿host en la lista? sí → túnel│
 └───────────────────────────────┘            │  └─ no → 403 y auditoría         │
                                              └──────────────────────────────────┘
```

* Dentro no hay ruta a ninguna parte: solo el puente hacia un socket Unix montado.
* El proxy del host solo acepta `CONNECT` a los dominios permitidos para ese backend (p. ej. la API
  de Anthropic para Claude Code). Cada conexión se audita.
* Requisito: que la CLI respete `HTTPS_PROXY`. Se verifica en F0.03 para cada una.

### Credenciales

Los directorios de configuración de cada CLI se montan en solo lectura en el `HOME` privado. Si la
CLI necesita escribir estado, se montan con su propio overlay y se descarta después. Los tokens por
variable de entorno solo pasan a la sandbox del backend que los necesita.

## Prototipo actual

[`tools/minisandbox/minisandbox.py`](../tools/minisandbox/minisandbox.py) implementa archivos, raíz
ro, `/tmp` y `HOME` privados, sin red o red abierta (`--red`), PIDs y tiempo. Sin Landlock, seccomp
ni red filtrada.

| Medida | Valor |
|---|---|
| Arranque de `true` (5 ejecuciones) | 53–66 ms, incluido el arranque de Python dos veces |
| Tests | 9 en verde: capa, aplicar, descartar, solo lectura fuera, escribible explícito, sin red, tiempo, PIDs, git |

## Requisitos y límites conocidos

| Situación | Efecto | Qué hacer |
|---|---|---|
| Ubuntu 24.04 o superior con `kernel.apparmor_restrict_unprivileged_userns=1` | los namespaces de usuario quedan sin capacidades | perfil AppArmor para `pcode` o `sysctl` a 0; si no, N2 |
| Dentro de Docker con el seccomp por defecto | `unshare(CLONE_NEWUSER)` denegado | N1 fuera del contenedor, o perfil seccomp propio; si no, N2 |
| Kernel < 5.11 | sin overlay en namespaces de usuario | `fuse-overlayfs` o N2 |
| Cambios en `.git/` dentro de la sandbox | se aplican también | no hacer commits dentro; en F3 los commits los hace la integración |
| QEMU/KVM (tareas `kernel` de SO_IA) | requiere `/dev/kvm` | N2 con `--device /dev/kvm`, como hoy |

## Amenazas que cubre y que no

| Cubre | No cubre (usa N3) |
|---|---|
| un agente que borra o reescribe el repo o el `HOME` | exploits del kernel |
| `git stash`, `reset --hard` o `clean` accidentales (solo afectan a la capa) | canales laterales |
| exfiltración por red (sin red, o solo dominios permitidos) | código malicioso que abuse de la API permitida |
| procesos huérfanos y bucles infinitos | agotar el disco de la capa (mitigado con cuota en F2.05) |

## Tareas

F2.01 (capa y cambios), F2.02 (ejecutar en namespaces), F2.03 (aplicar y descartar), F2.04
(Landlock), F2.05 (seccomp y límites), F2.19 (proxy de salida) y F2.20 (puente en la sandbox). Ver
[implementacion.md](implementacion.md).

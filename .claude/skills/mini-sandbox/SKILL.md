---
name: mini-sandbox
description: Ejecuta órdenes o experimentos en una mini sandbox (overlay copy-on-write sobre el repo, sin red, resto del disco en solo lectura) y después revisa, aplica o descarta los cambios. Úsala para probar algo arriesgado, comparar dos enfoques en paralelo o ejecutar código que no es de confianza.
---

# Mini sandbox (nivel N1)

Prototipo: `tools/minisandbox/minisandbox.py`. Necesita Linux con namespaces de usuario sin privilegios.

## Ejecutar

```bash
tools/minisandbox/minisandbox.py ejecutar --estado /tmp/ms-a -- cargo test -p pcode-core
```

Opciones:

- `--dir D`: directorio cubierto por el overlay (por defecto, el actual).
- `--red`: permite red (por defecto no hay).
- `--tiempo S`: segundos máximos (devuelve 124 si vence).
- `--escribible P`: ruta extra escribible fuera del repo, por ejemplo `~/.cargo`, para descargar
  crates. Repetible.

El estado (`--estado`) **no** puede estar dentro de `--dir`.

## Revisar

```bash
tools/minisandbox/minisandbox.py cambios /tmp/ms-a --diff
```

Lista lo añadido, modificado, borrado y reemplazado. Respeta `.gitignore` (`--incluir-ignorados`
para verlo todo).

## Decidir

```bash
tools/minisandbox/minisandbox.py aplicar /tmp/ms-a
```

```bash
tools/minisandbox/minisandbox.py descartar /tmp/ms-a
```

## Patrones

- **Dos enfoques en paralelo**: `--estado /tmp/ms-a` y `--estado /tmp/ms-b` sobre el mismo repo;
  aplica el que pase la aceptación y descarta el otro.
- **Probar sin miedo**: cualquier orden destructiva dentro de la sandbox solo afecta a la capa.
- **Tests**: `python3 -m unittest tools/minisandbox/test_minisandbox.py -v`.

## Límites conocidos (prototipo)

- No hay Landlock ni seccomp todavía (llegan en `crates/pcode-sandbox`, tareas F2.x).
- Dentro de Docker con el perfil seccomp por defecto puede no permitirse `unshare --user`. En ese
  caso usa el nivel N2 (contenedor).
- Los cambios en `.git/` también se aplican: no hagas commits dentro de la sandbox si no quieres
  copiarlos.

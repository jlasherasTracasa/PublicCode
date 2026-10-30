# ADR-0003: mini sandbox N1 como aislamiento por defecto

- Estado: aceptado
- Fecha: 2026-10-30

## Contexto

Lo habitual es un contenedor Docker o un `git clone` por tarea, mientras que las tareas pesadas
comparten el árbol y las reglas contra `git stash` o `reset --hard` son prosa. Necesitamos aislar cada orden y cada subagente de forma barata,
paralelizable y reversible. Además, la seguridad operativa de los agentes de código está en las
secuencias de acciones sobre un espacio de trabajo con estado (*SABER*, EMNLP 2026).

## Decisión

Nivel **N1** por defecto: namespaces de usuario, mount, PID y red + overlay copy-on-write sobre el
repo + resto en solo lectura + `/tmp` y `HOME` privados + Landlock + seccomp + límites, sin
privilegios. Red filtrada por un proxy de salida con lista de dominios. N2 (contenedor) para
toolchains completos y KVM; N3 (microVM) para código no confiable. El prototipo
`tools/minisandbox/minisandbox.py` valida el diseño: 9 tests y unos 55 ms de arranque.

## Alternativas

- **Docker para todo**: segundos por orden, capas de imagen y daemon con root; no escala a una
  sandbox por orden.
- **git worktree por subagente**: no aísla nada fuera del repo (`HOME`, red, procesos).
- **bubblewrap**: muy válido, pero es una dependencia externa; lo que usa son las mismas primitivas
  que podemos llamar desde Rust.
- **Firecracker para todo**: requiere KVM y es demasiado pesado para cada orden.

## Consecuencias

+ Paralelismo sin clones; deshacer = descartar la capa; las reglas destructivas pierden sentido
  (solo afectan a la capa).
+ La integración pasa a ser «aplicar diff + re-validar».
− Depende de que el kernel permita namespaces de usuario sin privilegios (AppArmor en Ubuntu 24.04,
  seccomp de Docker). Mitigación: detección al arrancar y caída a N2.

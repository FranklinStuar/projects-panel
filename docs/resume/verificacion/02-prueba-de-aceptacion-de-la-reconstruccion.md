# Prueba de aceptación de reconstrucción

> **Estado:** `TARGET`  
> Objetivo: demostrar que nueva implementación reproduce capacidades elegidas, mejora mantenibilidad y migra proyectos sin pérdida.

## Cómo usar este documento

Cada bloque define prueba observable, no intención. Resultado permitido:

- `PASS`: criterio completo y evidencia guardada.
- `FAIL`: comportamiento incorrecto o datos perdidos.
- `NOT-SELECTED`: función excluida explícitamente del alcance del nuevo producto.
- `BLOCKED`: dependencia externa impide prueba; requiere ticket y fecha.

No usar “parcialmente funciona” como PASS.

## A. Preparación limpia

### A1. Prerrequisitos

En máquina compatible y sin estado previo:

1. instalar dependencias declaradas;
2. compilar frontend y backend desde checkout limpio;
3. ejecutar first-run sin editar manualmente archivos generados;
4. confirmar red Docker, DNS wildcard, CA local y wrappers;
5. abrir aplicación.

**PASS si:** procedimiento documentado produce panel usable y una segunda ejecución es idempotente.

### A2. Arranque sin Docker

1. detener Docker;
2. abrir panel;
3. navegar UI y settings;
4. observar diagnóstico;
5. iniciar Docker y reintentar acción.

**PASS si:** app no colapsa, muestra causa accionable y recupera sin reinicio obligatorio cuando diseño lo permita.

## B. Fuente de verdad y compatibilidad

### B1. Descubrimiento de proyecto

1. colocar carpeta compatible con `config.json` bajo raíz configurada;
2. abrir/refrescar panel;
3. comprobar aparición y metadata.

**PASS si:** proyecto se descubre sin registro central.

### B2. Escritura atómica

1. forzar interrupción durante guardado de config;
2. reiniciar;
3. revisar archivo principal, temporal y journal.

**PASS si:** queda versión anterior o nueva completa; nunca JSON truncado.

### B3. Migración de schema

Probar config de cada versión soportada, incluyendo campos legacy Git.

**PASS si:** migración es determinista, auditable, idempotente y reversible mediante backup.

### B4. Importar instalación actual

1. copiar proyecto real del Panel WP anterior;
2. importar config, WordPress, dumps, snapshots y metadata relevante;
3. arrancar sin modificar origen;
4. comparar dominio, versiones, repos y exclusiones.

**PASS si:** no hay pérdida silenciosa y divergencias se muestran antes de confirmar.

## C. Lifecycle y recursos

### C1. Crear proyecto

Crear combinación soportada de:

- versión WordPress;
- versión PHP;
- motor/versión DB;
- SSL on/off;
- MinIO on/off.

**PASS si:** WordPress instalado, admin accesible, permisos host correctos, config persistida y operación deja journal finalizado.

### C2. Arranque idempotente

Invocar start dos veces concurrentes y una tercera después.

**PASS si:** un container PHP, un vhost y servicios compartidos sin duplicados.

### C3. Stop y cero recursos

1. encender dos proyectos que comparten DB;
2. detener uno;
3. verificar DB/nginx/mail siguen por segundo;
4. detener último.

**PASS si:** ningún container del panel queda activo cuando no corresponde; datos DB persisten.

### C4. Reconciliación tras apagón

Cortar aplicación y simular container/vhost huérfano.

**PASS si:** siguiente arranque detecta diferencia desired/actual, explica acción y repara sin perder DB.

### C5. Eliminar vs desconectar

Probar:

- desconectar conservando carpeta;
- borrar carpeta y schema;
- cancelar antes de fase destructiva;
- reimportar carpeta desconectada.

**PASS si:** cada opción afecta solo artefactos declarados y deja auditoría.

## D. Dominios, red y SSL

### D1. Puertos ocupados

Ocupar puertos candidatos antes de first start.

**PASS si:** selector elige par libre estable y mensaje identifica conflicto cuando endpoint persistido deja de estar disponible.

### D2. Wildcard `.test`

Resolver varios dominios no registrados individualmente.

**PASS si:** apuntan a loopback configurado sin romper DNS externo.

### D3. SSL

1. crear proyecto SSL;
2. abrir en navegador compatible;
3. regenerar certificado;
4. cambiar/expirar cert en escenario controlado.

**PASS si:** nginx recarga, cert corresponde al dominio y error ofrece reparación.

## E. Bases de datos y recuperación

### E1. Matriz de motores

Ejecutar creación, lectura, dump, import y drop para cada motor/version seleccionada.

**PASS si:** lifecycle y credenciales funcionan según contrato o motor queda explícitamente fuera de alcance.

### E2. Durabilidad

1. escribir datos;
2. recrear container DB;
3. reiniciar host/Docker;
4. volver a consultar.

**PASS si:** datos sobreviven y mount corresponde al datadir aprobado.

### E3. Import grande

Importar dump representativo con statement grande.

**PASS si:** progreso avanza, memoria se mantiene dentro del presupuesto, timeout no da falso positivo y DB final es consistente.

### E4. Interrupción de import

Matar operación a mitad y reiniciar.

**PASS si:** journal detecta operación incompleta y usuario puede reanudar desde cero o revertir a DB limpia; no se presenta éxito parcial.

### E5. Auto-dump

1. arrancar proyecto sin cambios;
2. esperar dos ciclos;
3. modificar DB;
4. esperar ciclo;
5. repetir sin modificar.

**PASS si:** no genera dumps idénticos, sí genera uno tras cambio, rota según política y registra origen.

### E6. Disaster recovery

Perder datadir de prueba y restaurar último dump siguiendo runbook.

**PASS si:** sitio recupera contenido esperado y procedimiento no depende de conocimiento no documentado.

## F. WordPress y herramientas

### F1. WP-CLI

Ejecutar desde UI y wrapper host:

```bash
wp core version
wp plugin list
wp user list
```

**PASS si:** corre como usuario no root correcto y archivos creados conservan ownership host.

### F2. Auto-login

Probar admin por defecto y usuario específico; reutilizar token y esperar expiración.

**PASS si:** primer uso funciona, segundo/expirado falla y redirect respeta permisos.

### F3. Terminal y VS Code

Abrir proyecto en terminal y workspace multi-root con repos detectados.

**PASS si:** cwd, wrapper y roots son correctos; workspace manual existente no se sobrescribe.

## G. Servicios compartidos

### G1. Mailpit

Enviar correo desde dos proyectos.

**PASS si:** un Mailpit captura ambos y permite distinguir origen según contrato.

### G2. MinIO

Activar solo en un proyecto y luego desactivarlo/detenerlo.

**PASS si:** arranca por demanda, persiste datos y se apaga cuando nadie lo necesita.

### G3. Adminer

Abrir DB de MySQL/MariaDB/Postgres seleccionado.

**PASS si:** apunta a container/schema correcto y no expone puerto fuera de interfaz permitida.

## H. Git, snapshots, clones y worktrees

### H1. Git

Probar scan, register, clone, pull, dirty, ahead/behind y remove en paths permitidos.

**PASS si:** no escapa de `public`, no pierde repos por normalización y errores Git son accionables.

### H2. Deploy directo local

Probar repo limpio y sucio, remoto adelantado, branch incorrecto, build exitoso/fallido y varios build dirs.

**PASS si:** solo hace fast-forward, no pisa cambios, journal identifica fase y fallo build no se presenta como deploy completo.

### H3. Punto de guardado

Crear snapshot con exclusiones y validar:

- `code.tar.zst` legible;
- `db.sql` importable;
- `meta.json` válido;
- uploads/cache/wp-config excluidos;
- exclusiones custom registradas.

### H4. Clone

Crear clone desde snapshot, editar DB/código y eliminarlo.

**PASS si:** padre no cambia, fallback uploads funciona, schema clone se elimina y snapshot permanece.

### H5. Worktree con DB compartida

Crear branch worktree, cambiar código y navegar dominio alterno.

**PASS si:** usa branch objetivo, DB padre no cambia URLs y borrar worktree conserva rama salvo elección explícita.

### H6. Worktree con DB copiada

Crear copia, modificar contenido y eliminar.

**PASS si:** DB padre queda intacta, schema derivado se elimina y mounts no dejan archivos huérfanos.

## I. Interfaces y paridad

### I1. IPC

Validar todos los comandos expuestos contra schema generado y errores tipados.

### I2. D-Bus/CLI/MCP

Para capacidad declarada en cada superficie:

1. ejecutar caso feliz;
2. pasar proyecto ambiguo/no existente;
3. provocar error backend;
4. comprobar formato y código de salida.

**PASS si:** adaptadores no reimplementan negocio y devuelven mismo resultado semántico.

### I3. Reactividad externa

Mutar proyecto por CLI y MCP mientras UI está abierta.

**PASS si:** UI refleja cambio mediante evento tipado sin refresh manual.

### I4. Concurrencia de progreso

Lanzar dos operaciones largas simultáneas.

**PASS si:** cada consola recibe solo eventos de su `operationId`; cancelar una no afecta otra.

## J. Seguridad

### J1. Paths

Probar `../`, symlinks, path absoluto y path fuera de proyecto en Git/build/snapshot/worktree.

**PASS si:** operación rechaza escape antes de mutar.

### J2. Comandos build

Probar comandos no permitidos según política nueva.

**PASS si:** alcance y aprobación son explícitos, argumentos quedan auditados y secretos no aparecen en logs.

### J3. Secretos y permisos

Revisar credenciales DB/MinIO, tokens y archivos de estado.

**PASS si:** permisos son mínimos, secretos no se serializan en config portable y rotación está documentada.

### J4. Límites de red

Comprobar bindings con `ss`/Docker inspect.

**PASS si:** servicios locales solo escuchan interfaces aprobadas y PHP/DB no publican puertos host.

## K. Observabilidad y mantenimiento

### K1. Logs estructurados

Correlacionar operación UI, caso de uso, comando host y acción Docker por `operationId`.

### K2. Diagnóstico

Provocar DNS roto, cert faltante, puerto ocupado, Docker inaccesible, vhost huérfano y wrapper viejo.

**PASS si:** diagnóstico identifica causa, evidencia y reparación sin inspección manual del código.

### K3. Retención

Aplicar retención de logs, journals, dumps y snapshots.

**PASS si:** política es configurable, no elimina fuera de scope y produce auditoría.

## L. Calidad y entrega

### L1. Gates automáticos

Obligatorios:

- format/lint;
- unit/domain tests;
- schema/contract tests;
- adapter tests;
- frontend component tests;
- e2e mock;
- integración Docker serial;
- smoke Tauri real para eventos/capabilities;
- validación de docs y links.

### L2. Presupuesto de recursos

Medir app inactiva, un proyecto y varios proyectos compartiendo servicios.

**PASS si:** cumple presupuesto definido y demuestra que compartir reduce consumo frente a duplicar.

### L3. Migración y rollback

1. migrar copia de instalación real;
2. operar nueva versión;
3. ejecutar rollback documentado.

**PASS si:** sistema anterior puede reabrir copia intacta o export compatible; no hay migración irreversible sin backup y confirmación.

## Criterio final

Reconstrucción se acepta cuando:

- todas capacidades seleccionadas tienen PASS;
- ningún FAIL afecta datos, seguridad o lifecycle;
- NOT-SELECTED está firmado como decisión de alcance;
- BLOCKED tiene responsable y fecha;
- matriz de trazabilidad enlaza requisito, caso de uso, adapter, estado, test y runbook;
- persona sin conocimiento previo ejecuta escenarios críticos usando solo documentación del repo.

## Evidencia a conservar

Por ejecución:

- commit/tag probado;
- plataforma y versiones;
- reporte de tests;
- logs de operaciones críticas;
- Docker inspect y medidas de recursos;
- checksums de dumps/snapshots usados;
- resultados de migración/rollback;
- lista firmada de capacidades NOT-SELECTED.

## Fuentes que motivan criterios

- `docs/TESTING.md`
- `src-tauri/src/integration_tests.rs`
- `src-tauri/src/docker.rs`
- `src-tauri/src/migrate.rs`
- `src-tauri/src/autodump.rs`
- `src-tauri/src/snapshot.rs`
- `src-tauri/src/worktree.rs`
- `src-tauri/capabilities/default.json`
- `docs/KNOWN_ISSUES.md`

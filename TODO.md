# TODO (próxima sesión)

## Grupos
- [x] Al arrastrar un nodo sobre un grupo, el grupo se agranda para alojarlo.
- [x] Renombrar un grupo debe ser más claro (hoy el título se edita sin indicación visible).
- [x] Evitar duplicados en "My groups": una bandera/huella que detecte un grupo guardado con los mismos módulos y parámetros y no lo agregue otra vez.
- [x] Clic derecho sobre un grupo guardado (barra lateral): cambiar su nombre. Hoy el clic derecho lo borra (`handle_right_press`, `src/main.rs`); decidir cómo se elimina entonces.
- [x] Clic derecho sobre un grupo del canvas: opción **Sincronizar con nodo favorito**, que resetea el grupo y sobrescribe el guardado.



## Canvas
- [x] Limitar el zoom a un máximo de 200 %.

## Math: Map y Clamp
- [x] Colapsar Map y Clamp en un solo nodo (Clamp ya es un modo de Map).
- [x] Botón de **autorango** en el nodo Map/Clamp: aprende mín/máx de la entrada.
- [x] Modo **centrar la entrada**.
- [x] Calibrar la entrada con una sección de **bias**.
- [ ] (Feature a charlar) **Modo sensor**: la descripción se cortó ("modo sensor ast…"). Falta que el usuario la complete antes de diseñarlo.

## Pendiente de la sesión anterior
- [x] Indicador visual de fuente sin señal en las previews (atenuar la preview cuando una cámara o vídeo lleva tiempo sin frames).
- [x] Mover a hilos aparte los diálogos de archivo (`rfd`) y el guardado de ajustes.
- [ ] Probar a mano: auto-insertar nodo sobre cable, empuje de nodos solapados, preview minimizada y recuperación de cámara USB desenchufada.
- [x] `favorite_shaders` en `settings.rs` ya no se usa; quitarlo.
- [x] que de los nodos no se listen todos los shaders sino que sea un unico nodo por categoria y que en este este el selector, a su vez que no aparezca el nombre con el ".glsl" al final sino el nombre.
- [x] crea un brownian, un shader de ripples y otros generadoresorganicos tipicos.

## Math: OSC (en pausa, sacado del menú)
El módulo ya existe en el código (`src/modules/math/osc.rs`, `ModuleKind::Osc`) pero no se puede crear desde la barra lateral hasta que madure. Falta:
- [ ] Volver a listarlo en `CATEGORIES` (Math) cuando esté listo.
- [ ] Filtro de dirección editable (hoy sigue la primera dirección que escucha; solo se resetea cambiando el puerto).
- [ ] Varias direcciones / varios valores por mensaje (hoy solo el primer argumento numérico).
- [ ] Botón para "reaprender" la dirección sin cambiar el puerto.
- [ ] Mostrar la IP local y el puerto para configurar el emisor.
- [ ] Probarlo a mano con TouchOSC / Max / Ableton y revisar el layout de sus cajas (Port, Min, Max) y el readout.
- [ ] Decidir si además hace falta un oscilador aparte (el LFO ya existe).

## Proyectos: vista compleja (no hecha todavía)
La vista simplificada ya existe: cada proyecto es independiente, aparece como pestaña sobre el canvas (`src/project.rs`), los ejemplos se abren como proyecto propio y al reabrir solo vuelve el último proyecto activo. Falta la **vista compleja**:
- [ ] Un canvas aparte, estilo KodeLife, donde cada proyecto es una caja/buffer con su salida de video y se conectan entre sí con cables.
- [ ] Un proyecto puede exponer su Output como buffer y otro proyecto leerlo como fuente (un nodo "Project In" que elige el proyecto).
- [ ] Evaluar los proyectos referenciados aunque no estén en la pestaña activa (hoy los proyectos aparcados no corren: sus cámaras y decodificadores se liberan).
- [ ] Orden de evaluación entre proyectos y detección de ciclos.
- [ ] Guardar/abrir esta vista (hoy la sesión solo guarda el proyecto activo).
- [ ] Botón o atajo para alternar entre vista simple y compleja.

## Map: modo Center (oculto)
El botón **Center** del Map se sacó de la interfaz por críptico, pero la implementación sigue (`MapState::center`, se guarda en el archivo y escala cada lado del cero por separado).
- [ ] Probarlo a mano (rango From con negativos y positivos, p. ej. -1..1 hacia 0..1) y decidir si vuelve con un nombre y una explicación más claros, o si se elimina.
- [ ] Para volver a mostrarlo: agregar `"Center"` a `MAP_BUTTONS` y devolver `map_button_rect` a tres botones por fila (`src/main.rs`).

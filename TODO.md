# TODO (próxima sesión)

## Grupos
- [ ] Al arrastrar un nodo sobre un grupo, el grupo se agranda para alojarlo.
- [ ] Renombrar un grupo debe ser más claro (hoy el título se edita sin indicación visible).
- [ ] Evitar duplicados en "My groups": una bandera/huella que detecte un grupo guardado con los mismos módulos y parámetros y no lo agregue otra vez.
- [ ] Clic derecho sobre un grupo guardado (barra lateral): cambiar su nombre. Hoy el clic derecho lo borra (`handle_right_press`, `src/main.rs`); decidir cómo se elimina entonces.
- [ ] Clic derecho sobre un grupo del canvas: opción **Sincronizar con nodo favorito**, que resetea el grupo y sobrescribe el guardado.



## Canvas
- [ ] Limitar el zoom a un máximo de 200 %.

## Math: Map y Clamp
- [ ] Colapsar Map y Clamp en un solo nodo (Clamp ya es un modo de Map).
- [ ] Botón de **autorango** en el nodo Map/Clamp: aprende mín/máx de la entrada.
- [ ] Modo **centrar la entrada**.
- [ ] Calibrar la entrada con una sección de **bias**.
- [ ] (Feature a charlar) **Modo sensor**: la descripción se cortó ("modo sensor ast…"). Falta que el usuario la complete antes de diseñarlo.

## Pendiente de la sesión anterior
- [ ] Indicador visual de fuente sin señal en las previews (atenuar la preview cuando una cámara o vídeo lleva tiempo sin frames).
- [ ] Mover a hilos aparte los diálogos de archivo (`rfd`) y el guardado de ajustes.
- [ ] Probar a mano: auto-insertar nodo sobre cable, empuje de nodos solapados, preview minimizada y recuperación de cámara USB desenchufada.
- [ ] `favorite_shaders` en `settings.rs` ya no se usa; quitarlo.
- [ ] que de los nodos no se listen todos los shaders sino que sea un unico nodo por categoria y que en este este el selector, a su vez que no aparezca el nombre con el ".glsl" al final sino el nombre.
- [ ] crea un brownian, un shader de ripples y otros generadoresorganicos tipicos.

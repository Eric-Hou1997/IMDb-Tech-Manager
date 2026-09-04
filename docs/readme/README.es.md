<div align="center">

# IMDb Tech Manager

[简体中文](../../README.md) | [繁體中文](./README.zh-Hant.md) | [English](./README.en.md) | [Français](./README.fr.md) | [Русский](./README.ru.md) | [日本語](./README.ja.md) | **Español** | [ไทย](./README.th.md)

<p align="center">

[![Versión](https://img.shields.io/github/v/release/Eric-Hou1997/IMDb-Tech-Manager?label=versión)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Descargas](https://img.shields.io/github/downloads/Eric-Hou1997/IMDb-Tech-Manager/total?label=descargas)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)
[![Estrellas](https://img.shields.io/github/stars/Eric-Hou1997/IMDb-Tech-Manager?style=flat\&logo=github)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/stargazers)
[![PR bienvenidas](https://img.shields.io/badge/PRs-bienvenidas-brightgreen.svg)](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/pulls)

</p>

<img src="../../macos/assets/ITM_logo.png" alt="IMDb Tech Manager" width="220">

Gestión de especificaciones técnicas y metadatos para bibliotecas de cine y contenido audiovisual.

---

## 🎬 Acerca del proyecto

**IMDb Tech Manager** es un proyecto centrado en gestionar información técnica de producciones cinematográficas y televisivas.

</p>

<img src="../images/poster.jpg" alt="Póster de IMDb Tech Manager" width="700">

</div>

</p>

El proyecto se ocupa de obtener, estructurar, normalizar y aplicar las **Especificaciones técnicas de IMDb**, para convertir información dispersa sobre producciones de cine y televisión en metadatos que puedan administrarse, buscarse y presentarse dentro de bibliotecas multimedia personales.

Las áreas de trabajo actuales incluyen:

* Especificaciones técnicas de IMDb
* Gestión de metadatos NFO
* Generación de etiquetas técnicas
* Información sobre cámaras y objetivos
* Formatos de captura en película y digital
* Formatos de producción y presentación
* Normalización de metadatos técnicos
* Presentación de información técnica en bibliotecas multimedia
* Procesamiento semántico asistido por IA
* Desarrollo asistido por Coding Agents

La mayoría de las bibliotecas multimedia ya ofrecen información excelente sobre títulos, reparto, años de estreno, resolución, códecs y formatos de audio.

Sin embargo, rara vez se conserva y presenta de manera completa y estructurada información como **qué cámaras y objetivos se utilizaron, qué formatos de captura en película o digital intervinieron, qué procesos de producción se emplearon y cómo se masterizó y presentó la obra final**.

IMDb Tech Manager pretende incorporar esta información al flujo de trabajo de las bibliotecas multimedia personales.

---

## 🖼️ Capturas de pantalla

### Gestión de datos

<div align="center">

<img src="../images/data-management.png" alt="Gestión de datos de IMDb Tech Manager" width="700">

</div>

</p>

La interfaz de gestión de datos NFO permite obtener Especificaciones técnicas de IMDb, organizar especificaciones técnicas, generar etiquetas y administrar tareas por lotes.

### Gestión de NFO

<div align="center">

<img src="../images/tag-management.png" alt="Gestión de etiquetas de IMDb Tech Manager" width="700">

</div>

</p>

La gestión de etiquetas permite inspeccionar, previsualizar y modificar los metadatos técnicos de la biblioteca multimedia, además de corregir manualmente el contenido generado de forma automática.

### Gestión del entorno de ejecución de IA

<div align="center">

<img src="../images/ai-runtime-management.png" alt="Gestión del entorno de ejecución de IA de IMDb Tech Manager" width="700">

</div>

</p>

La interfaz de gestión del entorno de ejecución de IA permite configurar endpoints de modelos, direcciones URL base de API, caché de prompts, parámetros de inferencia, prompts del sistema y otros comportamientos relacionados con la generación de etiquetas mediante IA.

### Especificaciones técnicas en la biblioteca multimedia

<div align="center">

<img src="../images/media-library-card.png" alt="Especificaciones técnicas de IMDb Tech Manager en la biblioteca multimedia" width="900">

</div>

</p>

Las especificaciones técnicas procesadas pueden utilizarse posteriormente para presentar información técnica dentro de las bibliotecas multimedia.

---

## 🔄 Flujo de trabajo principal

```text
Especificaciones técnicas de IMDb
        ↓
Obtención y estructuración de datos
        ↓
Normalización de especificaciones técnicas
        ↓
Gestión de metadatos NFO
        ↓
Generación de etiquetas técnicas
        ↓
Presentación de información técnica en bibliotecas multimedia
```

El objetivo no consiste simplemente en conservar el texto sin procesar de las páginas de IMDb. El proyecto busca transformar estas especificaciones técnicas en información estructurada que pueda:

* Administrarse
* Normalizarse
* Corregirse manualmente
* Buscarse
* Etiquetarse
* Presentarse
* Reutilizarse en otras herramientas

---

## 📚 Información técnica

Las Especificaciones técnicas de IMDb contienen una gran variedad de información sobre producciones cinematográficas y televisivas, entre ella:

* 📷 Cámaras
* 🔭 Objetivos
* 🎞️ Formatos de captura en película
* 💾 Formatos de captura digital
* 🎥 Procesos cinematográficos
* 🧪 Procesos de laboratorio y posproducción
* 🖼️ Relaciones de aspecto
* 🔊 Mezclas de sonido
* 📽️ Formatos de máster y presentación
* 🎬 Formatos de copia cinematográfica
* Otras Especificaciones técnicas relacionadas con la producción

IMDb Tech Manager desarrolla funciones de análisis, normalización, gestión de metadatos y presentación alrededor de esta información.

---

## 🧩 Arquitectura del proyecto

IMDb Tech Manager (ITM) y [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager) colaboran dentro del mismo flujo de **Especificaciones técnicas**.

Las dos herramientas se encargan de etapas diferentes:

* **ITM** obtiene, procesa, inspecciona y mantiene las especificaciones técnicas y los metadatos relacionados.
* **TCM** aplica las especificaciones técnicas procesadas a las bibliotecas multimedia —por ejemplo, generando las tarjetas correspondientes en la interfaz de Emby— y se ocupa de su presentación e integración.

ITM y TCM son herramientas independientes diseñadas para funcionar juntas, aunque también pueden utilizarse y desarrollarse por separado según el flujo de trabajo.

### 📦 IMDb Tech Manager (ITM)

**IMDb Tech Manager (ITM)** se encarga principalmente de gestionar y procesar los datos de Especificaciones técnicas.

Sus responsabilidades incluyen:

* Obtención de Especificaciones técnicas de IMDb
* Estructuración de Especificaciones técnicas
* Normalización de especificaciones técnicas
* Gestión de archivos NFO
* Escritura de Especificaciones técnicas en archivos NFO
* Generación de etiquetas técnicas
* Procesamiento semántico asistido por IA
* Vista previa / Simulación
* Corrección manual
* Procesamiento por lotes
* Mantenimiento de metadatos

ITM convierte las especificaciones técnicas sin procesar en metadatos multimedia estables, estructurados y mantenibles.

Las Especificaciones técnicas procesadas por ITM pueden ser utilizadas por **TCM** para generar, presentar y sincronizar tarjetas de especificaciones técnicas dentro de las bibliotecas multimedia.

### 🖥️ [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager)

**Tech Card Manager (TCM)** se encarga principalmente de presentar e integrar las Especificaciones técnicas dentro de las bibliotecas multimedia.

Sus responsabilidades incluyen:

* Leer Especificaciones técnicas suministradas por ITM u otras fuentes de datos compatibles
* Generar y mantener tarjetas de especificaciones técnicas
* Presentar información técnica
* Sincronizar metadatos
* Integrarse con la interfaz web
* Gestionar la compatibilidad entre distintos tipos de contenido
* Integrarse con flujos de metadatos de bibliotecas multimedia

TCM e ITM utilizan el mismo modelo de datos de Especificaciones técnicas, pero se centran en responsabilidades diferentes.

ITM obtiene, organiza y mantiene la información técnica; TCM aplica esos datos procesados a entornos reales de bibliotecas multimedia.

Juntos pueden formar un flujo de trabajo completo:

**IMDb → ITM → NFO / Especificaciones técnicas → TCM → Presentación en la biblioteca multimedia**

### 🧭 Relación entre plataformas

Las responsabilidades de ITM y TCM **no están vinculadas permanentemente a un sistema operativo ni a un servidor multimedia concretos**.

Las implementaciones disponibles o en desarrollo activo actualmente son:

* **ITM**: una aplicación de gestión de datos de Especificaciones técnicas centrada actualmente en **macOS**
* **TCM**: una herramienta de gestión de tarjetas de Especificaciones técnicas e integración con bibliotecas multimedia centrada actualmente en **Emby**

Estas son solo las implementaciones actuales y no implican que ninguno de los proyectos vaya a quedar limitado a estas plataformas.

La expansión futura puede incluir:

* Sistemas operativos adicionales
* Servidores multimedia adicionales
* Métodos de despliegue adicionales
* Aplicaciones cliente adicionales
* Fuentes de datos de Especificaciones técnicas compatibles adicionales

ITM y TCM mantienen una independencia relativa en el nivel arquitectónico y se comunican mediante Especificaciones técnicas y metadatos multimedia estandarizados, lo que deja margen para ampliarlos en distintas plataformas y entornos de biblioteca.

## 🧠 Principios de diseño

Uno de los objetivos fundamentales de desarrollo de IMDb Tech Manager es mantener la automatización **controlable, inspeccionable y recuperable**.

Tanto si una tarea utiliza reglas locales deterministas como IA, el objetivo es mantener de forma fiable los metadatos multimedia del usuario, no simplemente maximizar la automatización.

### 1. Preferencia por reglas deterministas

Cuando un problema puede resolverse de forma fiable con reglas explícitas, se prefiere la lógica local determinista.

Algunos ejemplos:

* Asignaciones de formatos conocidos
* Normalización de especificaciones técnicas
* Reglas de etiquetas
* Transformaciones de estructuras de datos
* Lectura y escritura de NFO
* Procesamiento de archivos
* Validación de datos
* Determinación de propiedad

Estas tareas tienen entradas y salidas claramente definidas, por lo que la lógica determinista resulta más fácil de probar, reproducir y verificar.

Las tareas que pueden resolverse de manera fiable mediante reglas no se delegan a la IA solo por utilizarla.

### 2. Uso de IA para semántica ambigua

La IA se utiliza principalmente para problemas semánticos difíciles de cubrir por completo con reglas fijas, como:

* Descripciones irregulares en lenguaje natural
* Especificaciones técnicas ambiguas
* Descomposición semántica compleja
* Identificación de relaciones entre fabricantes, series y modelos
* Normalización de distintas formas de expresar la misma información
* Información técnica que exige interpretación contextual

La IA es una capacidad más de IMDb Tech Manager.

Complementa las áreas donde las reglas deterministas son menos eficaces, sin sustituir el flujo general de procesamiento de datos.

### 3. Proveedores de IA sustituibles

La capa de modelos se mantiene configurable para poder conectar distintos servicios mediante parámetros como:

```text
Proveedor
URL base
Clave de API
Modelo
```

Esto permite cambiar de servicio de modelos según:

* Capacidad del modelo
* Coste de la API
* Disponibilidad
* Compatibilidad de la API
* Entorno de despliegue
* Requisitos de privacidad

El flujo principal de datos no queda vinculado permanentemente a un proveedor de IA concreto.

### 4. Vista previa antes de escribir

Siempre que sea posible, las operaciones que modifican archivos NFO o metadatos multimedia deben ofrecer un resultado inspeccionable antes de escribir realmente.

Flujo típico:

```text
Vista previa / Simulación
        ↓
Revisar cambios
        ↓
Ejecutar
        ↓
Verificar el resultado
```

Esto es especialmente importante para:

* Modificaciones de NFO
* Escritura de Especificaciones técnicas
* Generación de etiquetas técnicas
* Actualización de etiquetas técnicas
* Operaciones por lotes
* Migraciones de metadatos

En las operaciones por lotes, saber qué va a cambiar es más importante que limitarse a maximizar la velocidad de ejecución.

### 5. Respeto por los datos mantenidos por el usuario

IMDb Tech Manager distingue los metadatos según su origen y propiedad.

Los procesos automáticos solo pueden modificar datos que la aplicación pueda identificar de forma concluyente como propios. La semejanza textual nunca debe servir por sí sola para suponer que una etiqueta pertenece a IMDb Tech Manager.

En particular:

* Etiquetas mantenidas manualmente por los usuarios
* Etiquetas generadas por herramientas externas
* Datos mantenidos por aplicaciones como TMM
* Datos cuya propiedad se desconoce

no deben eliminarse, reclamarse ni sobrescribirse silenciosamente mediante tareas en segundo plano.

Una etiqueta técnica generada que el usuario haya editado manualmente se considera un dato protegido y mantenido por el usuario.

### 6. Separación entre Especificaciones técnicas y etiquetas

El proyecto trata las **Especificaciones técnicas** como capa de datos fácticos y las **Etiquetas** como información derivada de esa capa.

```text
Especificaciones técnicas
        ↓
Normalización / Procesamiento semántico
        ↓
Etiquetas técnicas
```

Por tanto:

* Spec Agent solo es responsable de las Especificaciones técnicas
* Los generadores de etiquetas son responsables de las Etiquetas técnicas
* Editar Especificaciones técnicas no debe iniciar implícitamente una actualización de IMDb
* La regeneración de etiquetas debe seguir siendo una operación independiente y observable

Esta separación evita que las distintas etapas de procesamiento interfieran entre sí y facilita regenerar etiquetas o integrar otras herramientas en el futuro.

### 7. Las escrituras deben ser seguras

Los archivos NFO son datos importantes y duraderos de la biblioteca multimedia, por lo que los flujos de modificación deben reducir al mínimo el riesgo de escrituras parciales, eliminación accidental de etiquetas o daños en los archivos.

El proyecto continúa reforzando:

* Comprobaciones de validez XML
* Copias de seguridad antes de modificar
* Comprobaciones del estado del archivo original
* Escrituras atómicas
* Validación de seguridad de rutas
* Verificación de propiedad de etiquetas
* Detección de conflictos
* Conservación intacta del archivo original si se produce un fallo
* Funciones de recuperación y deshacer

Cuando el sistema no puede determinar con seguridad si debe modificar un dato, de forma predeterminada omite la operación en lugar de arriesgarse a realizar una escritura insegura.

### 8. El comportamiento en segundo plano debe ser observable y verificable

Las operaciones importantes en segundo plano deben mostrar información clara sobre su estado siempre que sea posible.

Por ejemplo:

* Qué se está procesando
* Qué se ha modificado
* Qué resultados proceden de reglas locales
* Qué resultados proceden de la IA
* Número de llamadas a la API
* Uso de tokens
* Aciertos de caché
* Coste estimado
* Fase actual de la tarea
* Estados de éxito / fallo / omisión
* Motivos de error
* Títulos o archivos NFO afectados

El proyecto procura no reducir un trabajo complejo en segundo plano a un simple mensaje de «éxito» sin mostrar lo que realmente ha ocurrido.

---

## 🤖 Desarrollo con Coding Agents

El repositorio público de IMDb Tech Manager incluye:

[**`AGENTS.md` →**](../../AGENTS.md)

Este archivo constituye un punto de entrada contextual importante para Coding Agents como Codex al comprender y modificar el proyecto.

Actualmente, `AGENTS.md` abarca:

* Identidad del repositorio y límites del código fuente
* Responsabilidades actuales del producto
* Restricciones de arquitectura del software
* Reglas de seguridad de NFO
* Reglas de propiedad de etiquetas
* Reglas sobre llamadas de IA y contabilización de tokens
* Requisitos de gestión del ciclo de vida
* Requisitos de pruebas y verificación
* Límites de publicación
* Requisitos de firma OTA
* Límites de compatibilidad de plataformas
* Cambios que no deben realizarse
* Comprobaciones necesarias antes de completar una modificación

Después de hacer un fork o clonar el repositorio, los desarrolladores pueden pedir a un Coding Agent con acceso al contexto del repositorio que lea `AGENTS.md` antes de analizar o modificar el código.

Flujo recomendado:

```text
Fork / Clonar
        ↓
El Coding Agent lee AGENTS.md
        ↓
Leer el código fuente y las pruebas pertinentes
        ↓
Comprender responsabilidades y límites de los módulos
        ↓
Analizar los flujos funcionales afectados
        ↓
Crear un plan de implementación
        ↓
Modificar el código
        ↓
Ejecutar las pruebas pertinentes
        ↓
Verificar el comportamiento real
        ↓
Enviar un Pull Request
```

IMDb Tech Manager aspira a ofrecer algo más que el código fuente:

```text
Código fuente
    +
Conocimiento de la arquitectura
    +
Restricciones de diseño
    +
Reglas de desarrollo
    +
Métodos de prueba
    +
Contexto para agentes
```

Esto ayuda tanto a quienes leen directamente el código como a quienes trabajan con Coding Agents como Codex a comprender el proyecto más deprisa, y reduce el riesgo de introducir cambios que funcionen técnicamente pero infrinjan las restricciones de diseño existentes.

---

## 🚧 Estado actual

IMDb Tech Manager se encuentra actualmente en una fase de **código abierto y desarrollo activo**.

El historial público del código fuente comienza con **v4.0.0**.

El repositorio incluye actualmente:

* Código fuente completo del proyecto
* Implementación actual para macOS
* `AGENTS.md`
* Código de pruebas
* Herramientas de compilación de versiones
* Configuración de empaquetado
* Documentación del proyecto
* `LICENSE`
* `NOTICE`
* `SECURITY.md`
* [`PRIVACY.md`](../legal/PRIVACY.es.md)
* [`TERMS.md`](../legal/TERMS.es.md)

### Plataforma compatible actualmente

La implementación que se mantiene actualmente es:

**macOS · Apple Silicon (arm64)**

La aplicación de escritorio actual está compuesta principalmente por:

```text
Lanzador nativo
      +
Núcleo Go
      +
Interfaz web local
      +
Motor Python
```

El repositorio se organiza en torno al producto, no de forma permanente en torno a un sistema operativo.

Si en el futuro se admiten Windows u otras plataformas, seguirán formando parte de IMDb Tech Manager siempre que conserven las mismas responsabilidades de producto y el mismo flujo de datos.

### Versión oficial actual

Versión pública actual:

**IMDb Tech Manager v4.1.0**

La versión ofrece la aplicación `.app` para Apple Silicon dentro de un paquete ZIP junto con:

* Registro de cambios de la versión
* Información de verificación SHA-256
* Firma OTA Ed25519
* Instrucciones de instalación

[**Ver versiones →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases)

El chino simplificado, el chino tradicional y el inglés (Estados Unidos) están integrados. El francés, ruso, japonés, español y tailandés se ofrecen como paquetes de idioma verificados en la versión correspondiente de la aplicación. Consulta [`docs/language-packs.md`](../language-packs.md) para obtener más información.

El proyecto sigue en desarrollo activo; sus funciones, arquitectura, cobertura de pruebas y compatibilidad de plataformas continuarán evolucionando.

---

## 🗺️ Hoja de ruta

### Completado

* [x] Establecer el repositorio público
* [x] Publicar el código fuente principal
* [x] Establecer la estructura básica del proyecto
* [x] Publicar `AGENTS.md`
* [x] Seleccionar la licencia de código abierto
* [x] Establecer el marco inicial de pruebas
* [x] Establecer el flujo de compilación de versiones
* [x] Publicar la primera versión pública del código fuente, `v4.0.0`
* [x] Establecer el flujo de publicación para macOS Apple Silicon
* [x] Añadir verificación de integridad de versiones y firma OTA
* [x] Publicar `v4.1.0` con chino simplificado, chino tradicional e inglés (Estados Unidos) integrados
* [x] Publicar paquetes de idioma independientes para francés, ruso, japonés, español y tailandés
* [x] Compartir el estado del idioma entre la interfaz web, el núcleo Go, el motor Python y los menús nativos de macOS
* [x] Fijar al iniciar la tarea el idioma de los registros y la revisión, conservando registros históricos, cachés y prompts
* [x] Distinguir fallos de proxy/red, limitación de GitHub, recurso ausente, descarga y verificación de firma

### En curso

* [ ] Mejorar las reglas de normalización de Especificaciones técnicas
* [ ] Ampliar las funciones de procesamiento de datos de Especificaciones técnicas
* [ ] Mejorar la generación Local / IA de etiquetas técnicas
* [ ] Reforzar los mecanismos de propiedad y seguridad de datos NFO
* [ ] Ampliar las pruebas automatizadas y la cobertura de regresión en casos reales
* [ ] Mejorar el estado de las tareas, la localización de errores y la recuperación
* [ ] Mejorar la configuración de proveedores y modelos de IA
* [ ] Mejorar la contabilización de tokens, caché y costes de API
* [ ] Mejorar los flujos de actualización de la aplicación y de publicación
* [ ] Mejorar la firma con Developer ID y la experiencia de distribución en macOS
* [ ] Seguir mejorando `AGENTS.md` y el contexto para Coding Agents
* [ ] Mejorar los flujos de contribución y Pull Requests
* [ ] Explorar la compatibilidad con otros sistemas operativos
* [ ] Mejorar la integración del flujo de Especificaciones técnicas con [Tech Card Manager (TCM)](https://github.com/Eric-Hou1997/Tech-Card-Manager)
* [ ] Explorar la integración con otros servidores multimedia y aplicaciones cliente

La hoja de ruta seguirá evolucionando según el desarrollo del proyecto y la experiencia de uso real.

---

## 💬 Debates

En Discussions son bienvenidas las ideas de funciones, propuestas técnicas, diseño de interfaz, reglas de normalización de Especificaciones técnicas y flujos de desarrollo:

[**Ir a GitHub Discussions →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/discussions)

Algunos temas apropiados son:

* Ideas para nuevas funciones
* Debates sobre diseño técnico
* Reglas de datos de Especificaciones técnicas
* Información de cámaras / objetivos / película / formatos de producción
* Sugerencias de UI / UX
* Estrategias de generación de etiquetas con IA
* Flujos de trabajo de ITM y TCM
* Enfoques de desarrollo con Coding Agents
* Ideas que aún requieren exploración

Si una idea todavía necesita debate y validación antes de convertirse en una tarea bien definida, Discussions es un buen punto de partida antes de abrir un Issue.

---

## 🐛 Incidencias

Para problemas que ya puedan describirse con claridad, abre directamente un Issue:

[**GitHub Issues →**](https://github.com/Eric-Hou1997/IMDb-Tech-Manager/issues)

Por ejemplo:

* Errores reproducibles
* Funciones claramente ausentes
* Errores de análisis de datos
* Errores de normalización de Especificaciones técnicas
* Problemas al modificar NFO
* Problemas de comportamiento de la interfaz
* Problemas de publicación / instalación
* Solicitudes de funciones bien definidas

Siempre que sea posible, incluye la versión, el tipo de contenido, los pasos para reproducir el problema y la información del error para facilitar el diagnóstico.

---

## 🤝 Contribuciones

IMDb Tech Manager es de código abierto. Se aceptan forks, investigación, modificaciones y Pull Requests.

Antes de modificar el código, lee:

[**`AGENTS.md` →**](../../AGENTS.md)

Este archivo documenta reglas importantes de arquitectura, restricciones de seguridad de datos, requisitos de pruebas y límites de publicación.

En particular, comprende el diseño existente antes de modificar áreas como:

* Lectura y escritura de NFO
* Especificaciones técnicas
* Etiquetas técnicas
* Propiedad de etiquetas
* Llamadas de IA
* Contabilización de tokens / caché
* Tareas por lotes
* Ciclo de vida de la aplicación
* Código específico de cada plataforma
* Actualizaciones y publicaciones

Las contribuciones son bienvenidas, entre otras, en las áreas siguientes:

* Corrección de errores
* Mejoras de funciones
* Reglas de análisis de Especificaciones técnicas
* Normalización de especificaciones técnicas
* Información de cámaras / objetivos / formatos de producción
* Casos de prueba
* Mejoras de UI / UX
* Mejoras de rendimiento y estabilidad
* Documentación
* Mejoras del contexto para Coding Agents

Al cambiar un comportamiento existente, añade pruebas o verificaciones de regresión adecuadas siempre que resulte práctico, para evitar que la corrección de un problema rompa un flujo de metadatos multimedia ya existente.

---

## 📄 Licencia

IMDb Tech Manager es software de código abierto bajo la **Licencia Apache 2.0**.

Consulta la licencia completa:

[**LICENSE →**](../../LICENSE)

El repositorio también incluye:

[**NOTICE →**](../../NOTICE)

Al utilizar, modificar o distribuir el código fuente, cumple la Licencia Apache 2.0 y los avisos pertinentes incluidos en el repositorio.

---

## ⚠️ Aviso legal

IMDb Tech Manager es un proyecto de código abierto desarrollado de forma independiente.

Este proyecto **no está afiliado oficialmente, autorizado ni respaldado por IMDb, Emby ni ninguna otra plataforma de terceros**.

Todos los nombres, marcas, datos y servicios de terceros siguen siendo propiedad de sus respectivos titulares.

IMDb Tech Manager proporciona herramientas para obtener y procesar especificaciones técnicas y gestionar metadatos multimedia.

Los usuarios son responsables de garantizar que el uso que hagan de datos, API, sitios web y servicios de terceros cumpla las condiciones de servicio, los requisitos de licencia y las leyes aplicables.

---

## 💡 Comentarios y sugerencias

IMDb Tech Manager continúa en desarrollo activo.

Si tienes ideas relacionadas con:

* Especificaciones técnicas de IMDb
* Normalización de especificaciones técnicas
* Información sobre cámaras y objetivos
* Formatos de captura en película y digital
* Gestión de metadatos NFO
* Reglas de etiquetas técnicas
* Procesamiento semántico asistido por IA
* UI / UX
* Colaboración entre ITM y TCM
* Compatibilidad con sistemas operativos adicionales
* Flujos de desarrollo con Coding Agents

te invitamos a participar mediante Discussions o Issues.

El proyecto seguirá evolucionando sus funciones, reglas de datos y dirección de desarrollo a partir de la experiencia de uso real.

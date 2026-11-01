El asistente de IA es opcional: **no viene configurado**. Se activa en Ajustes → Asistente IA añadiendo un proveedor y su clave de API.

## Proveedores
- **Anthropic (Claude)**, **OpenAI**, **Google Gemini**, **OpenRouter**, **Groq**, **Mistral**, **DeepSeek**, **Together**, **xAI**, **Perplexity**, o cualquier servicio **compatible con OpenAI** (URL base propia).
- **Locales**: Ollama, LM Studio, llama.cpp, vLLM — sin clave si la URL es `localhost`.
- «Probar conexión» comprueba URL, clave y modelo; «Actualizar modelos» lee la lista del servicio. Se puede cambiar de proveedor o modelo en mitad de una conversación: se conserva el historial.
- Los modelos sin herramientas (`tools` desactivado) solo sirven de chat y ayuda: no tocan el show.

## Modos de confirmación
- **Preguntar**: confirma toda herramienta que cambie algo.
- **Inteligente** (por defecto): confirma solo lo que cambia lo que ve el público ahora mismo (GO, blackout, masters, escenas, valores en el programador sin blind) o es destructivo.
- **Automático**: no confirma nada.
Las confirmaciones caducan a los 5 minutos y cuentan como denegadas.

## Qué puede hacer
Leer el show y el estado, diagnosticar aparatos y DMX, buscar perfiles, ejecutar líneas de comandos y cualquier comando del protocolo, y preguntarte cuando haya ambigüedad. Los cambios en datos del show se agrupan y «Deshacer cambios del asistente» los revierte (no deshace lo lanzado en directo).

## Privacidad
- La clave se guarda solo en la máquina (`ai.json`, permisos 0600), nunca en el show ni en las copias, y no se envía de vuelta al navegador. También se puede usar la variable de entorno del proveedor (p. ej. `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`).
- Al hablar con el asistente se envía al proveedor elegido el texto de la conversación, el contexto de la interfaz y los datos del show que consulte. Con un modelo local no sale nada de la máquina.

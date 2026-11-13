//! The window's language: English, or Spanish. [`t`] gives a text in the language chosen
//! (the English text is the key; one not translated stays English).

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static SPANISH: AtomicBool = AtomicBool::new(false);

pub fn set_spanish(on: bool) {
    SPANISH.store(on, Ordering::Relaxed);
}

pub fn spanish() -> bool {
    SPANISH.load(Ordering::Relaxed)
}

const ES: &[(&str, &str)] = &[
    // the header
    ("PLAY", "TOCAR"),
    ("STOP", "PARAR"),
    ("NEXT", "SIGUIENTE"),
    ("SEED", "SEMILLA"),
    ("STAGE", "ESCENARIO"),
    ("EXPORT", "EXPORTAR"),
    ("MIDI", "MIDI"),
    ("LIVE", "DIRECTO"),
    ("ON AIR", "EN DIRECTO"),
    (
        "AMBIENT · DRONE · GENERATIVE",
        "AMBIENT · DRONE · GENERATIVO",
    ),
    // the layers and the director
    ("Drone", "Drone"),
    ("Pad", "Pad"),
    ("Texture", "Textura"),
    ("Bells", "Campanas"),
    ("Percussion", "Percusión"),
    ("Choir", "Coro"),
    ("Modular", "Modular"),
    ("The director", "El director"),
    ("Log & automations", "Registro y automatizaciones"),
    ("EVOLUTION", "EVOLUCIÓN"),
    ("PACE", "RITMO"),
    ("BRIGHTNESS", "BRILLO"),
    ("DENSITY", "DENSIDAD"),
    ("SPACE", "ESPACIO"),
    ("ECHO", "ECO"),
    ("WARMTH", "CALIDEZ"),
    ("VOLUME", "VOLUMEN"),
    ("When you play:", "Cuando tocas:"),
    ("Accompany", "Acompañar"),
    ("Solo", "Solo"),
    ("Relay", "Relevo"),
    ("Hold harmony", "Mantener armonía"),
    ("Sleep timer:", "Temporizador:"),
    ("Off", "No"),
    ("Nature:", "Naturaleza:"),
    ("none", "nada"),
    ("rain", "lluvia"),
    ("forest", "bosque"),
    ("fire", "fuego"),
    ("sea", "mar"),
    ("level", "nivel"),
    ("Lo-fi", "Lo-fi"),
    ("Binaural:", "Binaural:"),
    ("off", "no"),
    ("Follow the clock", "Seguir el reloj"),
    (
        "A soft beat, the crackle of a record, a darker sound",
        "Un ritmo suave, el crujido de un vinilo, un sonido más oscuro",
    ),
    (
        "A low tone in each ear, a few hertz apart. Use headphones.",
        "Un tono grave en cada oído, separados unos hercios. Usa auriculares.",
    ),
    (
        "The recipe changes with the time of day: mist at night, focus in the morning, dusk in the evening",
        "La receta cambia con la hora: niebla de noche, concentración por la mañana, atardecer por la tarde",
    ),
    ("Mute / unmute", "Silenciar / activar"),
    (
        "The MIDI keyboard plays this layer",
        "El teclado MIDI toca esta capa",
    ),
    ("ON", "ON"),
    // the LIVE window
    ("Go live", "Emitir en directo"),
    (
        "Stream the stage and the music to YouTube, without OBS.",
        "Emite el escenario y la música a YouTube, sin OBS.",
    ),
    ("Stream key:", "Clave de emisión:"),
    ("Server:", "Servidor:"),
    ("Quality:", "Calidad:"),
    ("Record instead:", "Grabar en su lugar:"),
    (
        "to a file (to test, or to keep)",
        "a un archivo (para probar o guardar)",
    ),
    (
        "YouTube Studio > Go live > Stream > Stream key",
        "YouTube Studio > Emitir en directo > Emisión > Clave de emisión",
    ),
    ("YouTube secure (RTMPS)", "YouTube seguro (RTMPS)"),
    (
        "Try it when a firewall or the network blocks the normal one",
        "Pruébalo si un cortafuegos o la red bloquean el normal",
    ),
    ("● Go live", "● Emitir"),
    ("■ Stop", "■ Parar"),
    ("Chat (YouTube)", "Chat (YouTube)"),
    ("API key:", "Clave de API:"),
    ("Live video:", "Vídeo en directo:"),
    (
        "the link of the live video",
        "el enlace del vídeo en directo",
    ),
    ("Votes (!orbit, !1)", "Votos (!orbit, !1)"),
    ("poll every", "encuesta cada"),
    (
        "Sounds (!nature rain, !lofi)",
        "Sonidos (!naturaleza lluvia, !lofi)",
    ),
    ("Welcome newcomers", "Saludar a los nuevos"),
    ("Blocked words:", "Palabras bloqueadas:"),
    (
        "more, comma-separated (insults are built in)",
        "más, separadas por comas (los insultos ya vienen)",
    ),
    ("Ignored names:", "Nombres ignorados:"),
    ("Connect to the chat", "Conectar al chat"),
    ("Disconnect", "Desconectar"),
    ("Try:", "Probar:"),
    ("Send", "Enviar"),
    ("Also stream to", "Emitir también a"),
    ("+ another", "+ otro"),
    ("stream key", "clave de emisión"),
    ("Schedule", "Horario"),
    ("Go live every day from", "Emitir cada día de"),
    ("to", "a"),
    ("Focus timer", "Temporizador de concentración"),
    ("Start 25 / 5", "Empezar 25 / 5"),
    ("Stop the timer", "Parar el temporizador"),
    (
        "On the stage: 25 minutes of focus, 5 of break.",
        "En el escenario: 25 minutos de concentración y 5 de pausa.",
    ),
    ("Tools", "Herramientas"),
    ("Make a thumbnail", "Crear miniatura"),
    (
        "A 1280×720 image of the stage now, with big words, for YouTube",
        "Una imagen 1280×720 del escenario ahora, con letras grandes, para YouTube",
    ),
    (
        "Record a 60 s Short (9:16)",
        "Grabar un Short de 60 s (9:16)",
    ),
    (
        "An upright video of the stage, for YouTube Shorts or TikTok",
        "Un vídeo vertical del escenario, para Shorts o TikTok",
    ),
    (
        "Cut the last recording into 1 h, 3 h and 8 h videos",
        "Cortar la última grabación en vídeos de 1, 3 y 8 h",
    ),
    (
        "Open the recordings folder",
        "Abrir la carpeta de grabaciones",
    ),
    ("Stage", "Escenario"),
    ("Day and night", "Día y noche"),
    (
        "The sky follows your clock: dawn, day, dusk, night",
        "El cielo sigue tu reloj: amanecer, día, atardecer, noche",
    ),
    ("Lake", "Lago"),
    ("Moving camera", "Cámara en movimiento"),
    ("Scenery", "Paisaje"),
    (
        "Each recipe's landscape: fog, a forest, a planet, arches…",
        "El paisaje de cada receta: niebla, bosque, planeta, arcos…",
    ),
    ("Weather", "Tiempo"),
    ("Seasons", "Estaciones"),
    ("Cozy window", "Ventana acogedora"),
    (
        "The stage seen through a rainy window, with a cup and a candle",
        "El escenario visto por una ventana con lluvia, una taza y una vela",
    ),
    ("Lumo", "Lumo"),
    (
        "The little lantern spirit on the hill",
        "El pequeño espíritu del farol en la colina",
    ),
    ("Seed constellation", "Constelación de la semilla"),
    (
        "Vertical video (9:16, for Shorts)",
        "Vídeo vertical (9:16, para Shorts)",
    ),
    ("Texts for YouTube", "Textos para YouTube"),
    ("Language:", "Idioma:"),
    ("Both", "Ambos"),
    ("Copy", "Copiar"),
    ("Title", "Título"),
    ("Description", "Descripción"),
    ("Pinned comment", "Comentario fijado"),
    ("Hashtags", "Hashtags"),
    ("Tags", "Etiquetas"),
    // the MIDI window
    ("Input:", "Entrada:"),
    (
        "MIDI learn: click Learn, then move a knob or fader of your controller.",
        "Aprendizaje MIDI: pulsa Learn y mueve un mando o fader de tu controlador.",
    ),
    (
        "Keys play the layer marked KEYS; channel 10 plays the percussion (drum pads).",
        "Las teclas tocan la capa marcada KEYS; el canal 10 toca la percusión (pads).",
    ),
    // the EXPORT window
    ("Export and record", "Exportar y grabar"),
    (
        "The music of this recipe and seed, from the start, as a file.",
        "La música de esta receta y semilla, desde el principio, en un archivo.",
    ),
    ("Length:", "Duración:"),
    ("Format:", "Formato:"),
    ("Export the mix", "Exportar la mezcla"),
    ("Export stems", "Exportar pistas"),
    (
        "One file per layer, to mix in another program",
        "Un archivo por capa, para mezclar en otro programa",
    ),
    (
        "Record what plays now, with what you play on the keyboard.",
        "Graba lo que suena ahora, con lo que toques en el teclado.",
    ),
    ("● Record", "● Grabar"),
    ("■ Stop recording", "■ Parar la grabación"),
    // my recipes
    ("My recipes", "Mis recetas"),
    ("Save as", "Guardar como"),
    ("Load", "Cargar"),
    ("Delete", "Borrar"),
    ("name", "nombre"),
    (
        "What plays now (the recipe, the knobs, the sounds), to come back to.",
        "Lo que suena ahora (la receta, los mandos, los sonidos), para volver a ello.",
    ),
    ("Update", "Actualizar"),
];

fn table() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| ES.iter().copied().collect())
}

/// `en` in the window's language.
pub fn t(en: &'static str) -> &'static str {
    if !spanish() {
        return en;
    }
    table().get(en).copied().unwrap_or(en)
}

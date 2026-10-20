// Spanish renderings of engine/REST error strings (the engine speaks English). Unknown messages pass through.
import { lang } from "./index";

const EXACT: Record<string, string> = {
  "Not connected": "Sin conexión con el motor",
  "Connection lost": "Conexión perdida",
  "Command failed": "El comando ha fallado",
  "Address must be 1..512": "La dirección debe estar entre 1 y 512",
  "Universe must be ≥ 1": "El universo debe ser ≥ 1",
  "Universe id must be ≥ 1": "El id de universo debe ser ≥ 1",
  "Only imported profiles can be removed (built-in ones are read-only)": "Solo se pueden eliminar perfiles importados (los integrados son de solo lectura)",
  "Invalid cue number": "Número de cue no válido",
  "Universe/channel out of range": "Universo o canal fuera de rango",
  "Invalid frame rate": "Frecuencia de fotogramas no válida",
  "Name required": "Se necesita un nombre",
  "Count must be 1..1024": "La cantidad debe estar entre 1 y 1024",
  "Mode has no channels": "El modo no tiene canales",
  "Nothing in the programmer for that palette type": "No hay nada en el programador para ese tipo de paleta",
  "Programmer is empty — nothing to record": "El programador está vacío: no hay nada que grabar",
  "Cuelist has no cues": "La lista de cues no tiene cues",
  "Unsupported file type (use .gdtf, .qxf or Open Fixture Library .json)": "Tipo de archivo no admitido (usa .gdtf, .qxf o .json de Open Fixture Library)",
  "Engine stopped": "El motor se ha detenido",
  "Invalid show name": "Nombre de show no válido",
  "Show name may only contain letters, digits, spaces and - _ . ( )": "El nombre del show solo puede contener letras, dígitos, espacios y - _ . ( )",
  "no serial port selected": "no hay puerto serie seleccionado",
  "built without usb support": "compilado sin soporte USB",
};

const NOUN: Record<string, string> = {
  Widget: "El widget", Group: "El grupo", Effect: "El efecto", Cuelist: "La lista de cues",
  Fixture: "El aparato", Palette: "La paleta", Cue: "El cue", Executor: "El ejecutor",
};

const PATTERNS: [RegExp, (m: RegExpMatchArray) => string][] = [
  [/^(Widget|Group|Effect|Cuelist|Fixture|Palette|Cue|Executor) (\S+) does not exist$/, (m) => `${NOUN[m[1]]} ${m[2]} no existe`],
  [/^Widget (\S+) is not an XY pad$/, (m) => `El widget ${m[1]} no es un pad XY`],
  [/^Profile has no mode '(.*)'$/, (m) => `El perfil no tiene el modo «${m[1]}»`],
  [/^Profile '(.*)' is used by patched fixtures$/, (m) => `El perfil «${m[1]}» está en uso por aparatos parcheados`],
  [/^Duplicate cue number (.*)$/, (m) => `Número de cue duplicado: ${m[1]}`],
  [/^Universe (\d+) still has (\d+) patched fixture\(s\)$/, (m) => `El universo ${m[1]} aún tiene ${m[2]} aparato(s) parcheado(s)`],
  [/^Unknown profile '(.*)'$/, (m) => `Perfil desconocido «${m[1]}»`],
  [/^Bad message: (.*)$/, (m) => `Mensaje no válido: ${m[1]}`],
  [/^Cannot write show: (.*)$/, (m) => `No se puede escribir el show: ${m[1]}`],
  [/^Cannot read show '(.*)': (.*)$/, (m) => `No se puede leer el show «${m[1]}»: ${m[2]}`],
  [/^Show '(.*)' is invalid: (.*)$/, (m) => `El show «${m[1]}» no es válido: ${m[2]}`],
  [/^Invalid JSON: (.*)$/, (m) => `JSON no válido: ${m[1]}`],
  [/^Not a valid GDTF \(zip\): (.*)$/, (m) => `No es un GDTF válido (zip): ${m[1]}`],
  [/^Import failed \((\d+)\)\s*(.*)$/, (m) => `La importación ha fallado (${m[1]}) ${m[2]}`.trim()],
  [/^unknown USB driver '(.*)'$/, (m) => `driver USB desconocido «${m[1]}»`],
];

/** Translate a server/REST error message for display. */
export function tErr(msg: string): string {
  if (lang.value !== "es") return msg;
  const e = EXACT[msg];
  if (e) return e;
  for (const [re, f] of PATTERNS) {
    const m = msg.match(re);
    if (m) return f(m);
  }
  return msg;
}

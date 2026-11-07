//! Interface language: English (the source text) or Hungarian.
//!
//! Every visible text in the code is the English sentence. `tr` looks it up in `TABLE` and gives the
//! Hungarian one when that language is on; a text that is not in the table stays English.
//! Texts with values use `{}` places, filled in order by `fill`.
use crate::config::Language;
use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};

static HUNGARIAN: AtomicBool = AtomicBool::new(false);

/// Switches the interface to `lang`; `Auto` follows the Windows display language.
pub fn apply(lang: Language) {
    let hu = match lang {
        Language::Auto => system_is_hungarian(),
        Language::English => false,
        Language::Hungarian => true,
    };
    HUNGARIAN.store(hu, Ordering::Relaxed);
}

pub fn is_hungarian() -> bool {
    HUNGARIAN.load(Ordering::Relaxed)
}

fn system_is_hungarian() -> bool {
    const LANG_HUNGARIAN: u16 = 0x0e;
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    id & 0x3ff == LANG_HUNGARIAN
}

/// The text in the current language.
pub fn tr(s: &'static str) -> &'static str {
    if !is_hungarian() {
        return s;
    }
    TABLE.iter().find(|(en, _)| *en == s).map(|(_, hu)| *hu).unwrap_or(s)
}

/// Like `tr` for a text that is only known at run time (an error message that may be in the table).
pub fn tr_str(s: &str) -> String {
    if is_hungarian() {
        if let Some((_, hu)) = TABLE.iter().find(|(en, _)| *en == s) {
            return (*hu).to_string();
        }
    }
    s.to_string()
}

/// Fills the `{}` places of `template` with `args`, in order.
pub fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    let mut i = 0;
    while let Some(p) = rest.find("{}") {
        out.push_str(&rest[..p]);
        if let Some(a) = args.get(i) {
            out.push_str(&a.to_string());
        }
        i += 1;
        rest = &rest[p + 2..];
    }
    out.push_str(rest);
    out
}

/// (English, Hungarian)
const TABLE: &[(&str, &str)] = &[
    // main window
    ("Ready to record", "Kész a felvételre"),
    ("Choose something to record", "Válaszd ki, mit rögzítsünk"),
    ("Preferences", "Beállítások"),
    ("●  Record   {}", "●  Felvétel   {}"),
    ("Last recording", "Utolsó felvétel"),
    ("Make GIF", "GIF készítése"),
    ("An animated GIF of the last recording (up to 30 s, 640 px wide)", "Animált GIF az utolsó felvételből (legfeljebb 30 mp, 640 px széles)"),
    ("Screen", "Képernyő"),
    ("{} x {} at {} fps", "{} x {}, {} fps"),
    ("Off", "Ki"),
    ("System audio", "Rendszerhang"),
    ("Stage", "Színpad"),
    ("Drag the frame, or its edges, to choose what gets recorded.", "Húzd a keretet vagy a szélét, hogy kijelöld, mi kerüljön a felvételre."),
    ("Screen {}", "Képernyő {}"),
    ("Draw on screen", "Kijelölés a képernyőn"),
    ("Width", "Szélesség"),
    ("Height", "Magasság"),
    ("Ratio", "Arány"),
    ("Free", "Szabad"),
    ("Size", "Méret"),
    ("Custom ({} x {})", "Egyéni ({} x {})"),
    ("Follow", "Követés"),
    ("The recorded frame moves with the window it was picked from", "A felvételi keret mozog azzal az ablakkal, amelyből kijelölted"),
    // preferences
    ("Close", "Bezárás"),
    ("General", "Általános"),
    ("Inputs", "Bemenetek"),
    ("Files", "Fájlok"),
    ("Shortcuts", "Gyorsbillentyűk"),
    ("Language", "Nyelv"),
    ("Auto", "Automatikus"),
    ("Show recorder countdown", "Visszaszámlálás mutatása"),
    ("Do not capture recorder on screen", "A felvevő ne kerüljön a felvételre"),
    ("Restore cursor location after pause", "Egér helyének visszaállítása szünet után"),
    ("Show the mouse pointer in the recording", "Egérmutató a felvételen"),
    ("Highlight the pointer and clicks", "Mutató és kattintások kiemelése"),
    ("Exit after stopped", "Kilépés leállítás után"),
    ("Open recording after stopped", "Felvétel megnyitása leállítás után"),
    ("Show region indicator", "Területjelző keret mutatása"),
    ("Automatically minimize recording window", "Felvételi ablak automatikus lekicsinyítése"),
    ("Maximum frame rate", "Legnagyobb képkockasebesség"),
    ("Quality", "Minőség"),
    ("Start a new file every", "Új fájl indítása minden"),
    (" min", " perc"),
    ("(0 = one file)", "(0 = egy fájl)"),
    ("Use the graphics card to encode, when possible", "Videokártya használata kódoláshoz, ha lehet"),
    ("Encoder of the last recording: {}", "Az utolsó felvétel kódolója: {}"),
    ("Name", "Név"),
    ("Automatic file name", "Automatikus fájlnév"),
    ("Prompt for file name", "Fájlnév bekérése"),
    ("File name template", "Fájlnévsablon"),
    ("{date}  {time}  {window}  (the window picked on screen, or \"screen\")", "{date}  {time}  {window}  (a képernyőn kijelölt ablak, vagy „screen”)"),
    ("Default output folder", "Alapértelmezett kimeneti mappa"),
    ("Temporary storage folder", "Ideiglenes tárolómappa"),
    ("Browse", "Tallózás"),
    ("Record, pause and resume", "Indítás, szünet és folytatás"),
    ("Stop", "Leállítás"),
    ("Add marker", "Jelölő hozzáadása"),
    ("Press a key...", "Nyomj egy billentyűt..."),
    ("Restore defaults", "Alapértékek visszaállítása"),
    ("Could not assign the following shortcuts: {}", "Nem sikerült kiosztani ezeket a gyorsbillentyűket: {}"),
    ("{} can't be used as a shortcut", "A(z) {} nem használható gyorsbillentyűnek"),
    ("The shortcut {} conflicts with {}", "A(z) {} gyorsbillentyű ütközik ezzel: {}"),
    // questions and the recording bar
    ("Recover recording", "Felvétel helyreállítása"),
    ("A recording wasn't finished the last time. Keep what was recorded?", "Egy felvétel nem fejeződött be legutóbb. Megtartod, ami fel lett véve?"),
    ("Recover", "Helyreállítás"),
    ("Delete", "Törlés"),
    ("Save recording", "Felvétel mentése"),
    ("The recording hasn't been saved yet.", "A felvétel még nincs elmentve."),
    ("Save", "Mentés"),
    ("MP4 video", "MP4 videó"),
    ("Are you sure you want to delete this recording and quit?", "Biztosan törlöd ezt a felvételt, és kilépsz?"),
    ("Are you sure you want to delete this recording?", "Biztosan törlöd ezt a felvételt?"),
    ("Yes", "Igen"),
    ("No", "Nem"),
    ("Pause", "Szünet"),
    ("Resume", "Folytatás"),
    ("Marker", "Jelölő"),
    ("Restart", "Újrakezdés"),
    ("Delete this recording and quit", "Felvétel törlése és kilépés"),
    ("Press {} to stop the recording.", "A felvétel leállításához nyomd meg ezt: {}"),
    ("Click a window or screen, or drag a rectangle. Esc cancels.", "Kattints egy ablakra vagy képernyőre, vagy húzz egy téglalapot. Esc: mégse."),
    ("Paused", "Szünet"),
    // messages
    ("Turn on the screen to record.", "Kapcsold be a képernyőt a felvételhez."),
    ("The recording could not start: {}", "A felvétel nem indulhatott el: {}"),
    ("Making the GIF...", "GIF készítése..."),
    ("GIF saved: {}", "GIF mentve: {}"),
    ("The GIF could not be made: {}", "A GIF-et nem sikerült elkészíteni: {}"),
    ("An error occurred in the Recorder: {}. The recording can be recovered at the next start.", "Hiba történt a rögzítőben: {}. A felvétel a következő indításkor helyreállítható."),
    ("An error occurred in the Recorder: {}. It stopped and saved the current recording.", "Hiba történt a rögzítőben: {}. A felvétel leállt, és az addigi rész mentve lett."),
    ("The output folder isn't available ({}). The recording can be recovered at the next start.", "A kimeneti mappa nem érhető el ({}). A felvétel a következő indításkor helyreállítható."),
    ("The recording could not be saved ({}). It can be recovered at the next start.", "A felvételt nem sikerült menteni ({}). A következő indításkor helyreállítható."),
    ("Unable to record system audio. Please connect or enable a playback device and try again.", "A rendszerhang rögzítése nem lehetséges. Csatlakoztass vagy engedélyezz egy lejátszóeszközt, és próbáld újra."),
    ("The screen could not be read.", "A képernyőt nem sikerült beolvasni."),
    // errors from the recorder
    ("Video Write Failed", "A videó írása sikertelen"),
    ("Audio Write Failed", "A hang írása sikertelen"),
    ("Invalid Input", "Érvénytelen bemenet"),
    ("Not Enough Space On Disk", "Nincs elég hely a lemezen"),
    ("Could not start the next part: {}", "Nem sikerült elindítani a következő részt: {}"),
    // tray
    ("Show mnRekord", "mnRekord megnyitása"),
    ("Start recording", "Felvétel indítása"),
    ("Stop recording", "Felvétel leállítása"),
    ("Quit", "Kilépés"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hungarian_text_has_the_same_places_as_the_english() {
        for (en, hu) in TABLE {
            assert!(!hu.is_empty(), "{en}");
            assert_eq!(en.matches("{}").count(), hu.matches("{}").count(), "places differ in {en:?}");
        }
    }

    #[test]
    fn no_english_text_is_listed_twice() {
        for (i, (en, _)) in TABLE.iter().enumerate() {
            assert!(!TABLE[..i].iter().any(|(e, _)| e == en), "{en:?} twice");
        }
    }

    #[test]
    fn fill_puts_the_values_in_order() {
        assert_eq!(fill("{} x {} at {} fps", &[&1920, &1080, &30]), "1920 x 1080 at 30 fps");
        assert_eq!(fill("no places", &[&1]), "no places");
        assert_eq!(fill("{} and {}", &[&"a"]), "a and ");
    }

    #[test]
    fn language_switch_changes_the_text() {
        apply(Language::Hungarian);
        assert_eq!(tr("Pause"), "Szünet");
        assert_eq!(tr("not in the table"), "not in the table");
        assert_eq!(tr_str("Invalid Input"), "Érvénytelen bemenet");
        apply(Language::English);
        assert_eq!(tr("Pause"), "Pause");
    }
}

//! The deck's own words in the language the project is written in: detected from its text, or
//! named by the caller. Templates take their values for `{}` in order.

use crate::model::{Document, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Pt,
    Es,
    Fr,
    De,
    It,
}

/// Things the deck counts.
#[derive(Clone, Copy)]
pub enum Noun {
    Screen,
    Board,
    Page,
    State,
    OpenQuestion,
}

pub struct Words {
    pub presentation: &'static str,
    pub contents: &'static str,
    pub at_a_glance: &'static str,
    pub open_questions: &'static str,
    pub next_steps: &'static str,
    pub appendix: &'static str,
    pub appendix_states: &'static str,
    pub other_states: &'static str,
    pub thank_you: &'static str,
    pub answer: &'static str,
    pub answer_detail: &'static str,
    pub review: &'static str,
    pub and: &'static str,
    pub review_detail: &'static str,
    pub check_states: &'static str,
    pub check_states_detail: &'static str,
    pub send_feedback: &'static str,
    pub to: &'static str,
    pub notes: &'static str,
    pub in_appendix: &'static str,
    /// Singular and plural of each [`Noun`], in its order.
    nouns: [(&'static str, &'static str); 5],
    months: [&'static str; 12],
    /// Day, month name, year.
    date: &'static str,
}

/// `template` with each `{}` replaced by the next of `values`.
pub fn fill(template: &str, values: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = template;
    for v in values {
        let Some((head, tail)) = rest.split_once("{}") else { break };
        out.push_str(head);
        out.push_str(v);
        rest = tail;
    }
    out.push_str(rest);
    out
}

impl Words {
    pub fn count(&self, n: usize, what: Noun) -> String {
        let (one, many) = self.nouns[what as usize];
        format!("{n} {}", if n == 1 { one } else { many })
    }

    /// The date of Unix milliseconds (UTC; civil-from-days), as the language writes it.
    pub fn date(&self, ms: u64) -> String {
        let z = (ms / 86_400_000) as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        fill(self.date, &[&d.to_string(), self.months[(m - 1) as usize], &y.to_string()])
    }
}

impl Lang {
    /// `en`, `pt`, `pt-BR`, `es`, `fr`, `de` or `it`.
    pub fn parse(code: &str) -> Option<Lang> {
        let base = code.split(['-', '_']).next()?.to_ascii_lowercase();
        Some(match base.as_str() {
            "en" => Lang::En,
            "pt" => Lang::Pt,
            "es" => Lang::Es,
            "fr" => Lang::Fr,
            "de" => Lang::De,
            "it" => Lang::It,
            _ => return None,
        })
    }

    /// The language most of the documents' text is written in; English when nothing stands out.
    pub fn detect<'a>(docs: impl IntoIterator<Item = &'a Document>) -> Lang {
        // Common words, and letters only some of the languages use.
        const CUES: [(Lang, &[&str], &str); 6] = [
            (Lang::En, &["the", "and", "of", "to", "is", "with", "for", "your", "you", "this", "on"], ""),
            (Lang::Pt, &["de", "da", "do", "que", "para", "com", "não", "uma", "os", "das", "dos", "em", "você", "seu", "sua"], "ãõç"),
            (Lang::Es, &["de", "la", "el", "que", "en", "los", "para", "con", "una", "del", "las", "tu", "su"], "ñ¿¡"),
            (Lang::Fr, &["le", "la", "les", "de", "des", "et", "pour", "avec", "une", "du", "vous", "est", "votre"], "èêëœ"),
            (Lang::De, &["der", "die", "und", "das", "mit", "für", "ist", "ein", "eine", "nicht", "zu", "den", "ihre"], "äöüß"),
            (Lang::It, &["il", "di", "che", "per", "con", "una", "gli", "della", "del", "non", "le", "sono"], "ìòù"),
        ];
        const LETTER_WEIGHT: usize = 3;
        let mut score = [0usize; 6];
        for doc in docs {
            for n in doc.nodes.values() {
                let (NodeKind::Text { text, .. } | NodeKind::Sticky { text, .. }) = &n.kind else { continue };
                let lower = text.to_lowercase();
                for word in lower.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()) {
                    for (i, (_, words, _)) in CUES.iter().enumerate() {
                        score[i] += usize::from(words.contains(&word));
                    }
                }
                for c in lower.chars() {
                    for (i, (_, _, letters)) in CUES.iter().enumerate() {
                        score[i] += LETTER_WEIGHT * usize::from(letters.contains(c));
                    }
                }
            }
        }
        let best = (0..CUES.len()).max_by_key(|i| (score[*i], std::cmp::Reverse(*i))).unwrap_or(0);
        if score[best] == 0 { Lang::En } else { CUES[best].0 }
    }

    pub fn words(self) -> &'static Words {
        match self {
            Lang::En => &EN,
            Lang::Pt => &PT,
            Lang::Es => &ES,
            Lang::Fr => &FR,
            Lang::De => &DE,
            Lang::It => &IT,
        }
    }
}

static EN: Words = Words {
    presentation: "DESIGN PRESENTATION",
    contents: "Contents",
    at_a_glance: "At a glance",
    open_questions: "Open questions",
    next_steps: "Next steps",
    appendix: "Appendix",
    appendix_states: "Appendix \u{b7} states",
    other_states: "Other states of the screens",
    thank_you: "Thank you",
    answer: "Answer {}",
    answer_detail: "From the notes on the screens; slide {} on",
    review: "Review the {}",
    and: "{} and {}",
    review_detail: "Feedback on a screen by its name, as titled on its slide",
    check_states: "Check the other states",
    check_states_detail: "The states kept out of the main flow; appendix, slide {} on",
    send_feedback: "Send feedback",
    to: "To {}",
    notes: "notes {}/{}",
    in_appendix: "+ {} in the appendix",
    nouns: [("screen", "screens"), ("board", "boards"), ("page", "pages"), ("state", "states"), ("open question", "open questions")],
    months: ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
    date: "{} {} {}",
};

static PT: Words = Words {
    presentation: "APRESENTA\u{c7}\u{c3}O DE DESIGN",
    contents: "Sum\u{e1}rio",
    at_a_glance: "Vis\u{e3}o geral",
    open_questions: "Perguntas em aberto",
    next_steps: "Pr\u{f3}ximos passos",
    appendix: "Ap\u{ea}ndice",
    appendix_states: "Ap\u{ea}ndice \u{b7} estados",
    other_states: "Outros estados das telas",
    thank_you: "Obrigado",
    answer: "Responder {}",
    answer_detail: "Das notas nas telas; a partir do slide {}",
    review: "Revisar {}",
    and: "{} e {}",
    review_detail: "Coment\u{e1}rios sobre uma tela pelo nome, como no t\u{ed}tulo do slide",
    check_states: "Ver os outros estados",
    check_states_detail: "Os estados fora do fluxo principal; ap\u{ea}ndice, a partir do slide {}",
    send_feedback: "Enviar coment\u{e1}rios",
    to: "Para {}",
    notes: "notas {}/{}",
    in_appendix: "+ {} no ap\u{ea}ndice",
    nouns: [("tela", "telas"), ("quadro", "quadros"), ("p\u{e1}gina", "p\u{e1}ginas"), ("estado", "estados"), ("pergunta em aberto", "perguntas em aberto")],
    months: ["janeiro", "fevereiro", "mar\u{e7}o", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"],
    date: "{} de {} de {}",
};

static ES: Words = Words {
    presentation: "PRESENTACI\u{d3}N DE DISE\u{d1}O",
    contents: "\u{cd}ndice",
    at_a_glance: "De un vistazo",
    open_questions: "Preguntas abiertas",
    next_steps: "Pr\u{f3}ximos pasos",
    appendix: "Ap\u{e9}ndice",
    appendix_states: "Ap\u{e9}ndice \u{b7} estados",
    other_states: "Otros estados de las pantallas",
    thank_you: "Gracias",
    answer: "Responder {}",
    answer_detail: "De las notas en las pantallas; desde la diapositiva {}",
    review: "Revisar {}",
    and: "{} y {}",
    review_detail: "Comentarios sobre una pantalla por su nombre, como se titula en su diapositiva",
    check_states: "Revisar los otros estados",
    check_states_detail: "Los estados fuera del flujo principal; ap\u{e9}ndice, desde la diapositiva {}",
    send_feedback: "Enviar comentarios",
    to: "Para {}",
    notes: "notas {}/{}",
    in_appendix: "+ {} en el ap\u{e9}ndice",
    nouns: [("pantalla", "pantallas"), ("tablero", "tableros"), ("p\u{e1}gina", "p\u{e1}ginas"), ("estado", "estados"), ("pregunta abierta", "preguntas abiertas")],
    months: ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
    date: "{} de {} de {}",
};

static FR: Words = Words {
    presentation: "PR\u{c9}SENTATION DESIGN",
    contents: "Sommaire",
    at_a_glance: "En bref",
    open_questions: "Questions ouvertes",
    next_steps: "Prochaines \u{e9}tapes",
    appendix: "Annexe",
    appendix_states: "Annexe \u{b7} \u{e9}tats",
    other_states: "Autres \u{e9}tats des \u{e9}crans",
    thank_you: "Merci",
    answer: "R\u{e9}pondre \u{e0} {}",
    answer_detail: "D\u{2019}apr\u{e8}s les notes des \u{e9}crans ; \u{e0} partir de la diapositive {}",
    review: "Relire {}",
    and: "{} et {}",
    review_detail: "Vos retours sur un \u{e9}cran par son nom, tel qu\u{2019}il est titr\u{e9} sur sa diapositive",
    check_states: "Voir les autres \u{e9}tats",
    check_states_detail: "Les \u{e9}tats hors du parcours principal ; annexe, \u{e0} partir de la diapositive {}",
    send_feedback: "Envoyer vos retours",
    to: "\u{c0} {}",
    notes: "notes {}/{}",
    in_appendix: "+ {} en annexe",
    nouns: [("\u{e9}cran", "\u{e9}crans"), ("tableau", "tableaux"), ("page", "pages"), ("\u{e9}tat", "\u{e9}tats"), ("question ouverte", "questions ouvertes")],
    months: ["janvier", "f\u{e9}vrier", "mars", "avril", "mai", "juin", "juillet", "ao\u{fb}t", "septembre", "octobre", "novembre", "d\u{e9}cembre"],
    date: "{} {} {}",
};

static DE: Words = Words {
    presentation: "DESIGN-PR\u{c4}SENTATION",
    contents: "Inhalt",
    at_a_glance: "Auf einen Blick",
    open_questions: "Offene Fragen",
    next_steps: "N\u{e4}chste Schritte",
    appendix: "Anhang",
    appendix_states: "Anhang \u{b7} Zust\u{e4}nde",
    other_states: "Weitere Zust\u{e4}nde der Screens",
    thank_you: "Vielen Dank",
    answer: "{} beantworten",
    answer_detail: "Aus den Notizen auf den Screens; ab Folie {}",
    review: "{} pr\u{fc}fen",
    and: "{} und {}",
    review_detail: "Feedback zu einem Screen mit seinem Namen, wie auf seiner Folie betitelt",
    check_states: "Die weiteren Zust\u{e4}nde ansehen",
    check_states_detail: "Die Zust\u{e4}nde au\u{df}erhalb des Hauptablaufs; Anhang, ab Folie {}",
    send_feedback: "Feedback senden",
    to: "An {}",
    notes: "Notizen {}/{}",
    in_appendix: "+ {} im Anhang",
    nouns: [("Screen", "Screens"), ("Board", "Boards"), ("Seite", "Seiten"), ("Zustand", "Zust\u{e4}nde"), ("offene Frage", "offene Fragen")],
    months: ["Januar", "Februar", "M\u{e4}rz", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
    date: "{}. {} {}",
};

static IT: Words = Words {
    presentation: "PRESENTAZIONE DI DESIGN",
    contents: "Indice",
    at_a_glance: "In sintesi",
    open_questions: "Domande aperte",
    next_steps: "Prossimi passi",
    appendix: "Appendice",
    appendix_states: "Appendice \u{b7} stati",
    other_states: "Altri stati delle schermate",
    thank_you: "Grazie",
    answer: "Rispondere a {}",
    answer_detail: "Dalle note sulle schermate; dalla slide {}",
    review: "Rivedere {}",
    and: "{} e {}",
    review_detail: "Feedback su una schermata con il suo nome, come nel titolo della slide",
    check_states: "Controllare gli altri stati",
    check_states_detail: "Gli stati fuori dal flusso principale; appendice, dalla slide {}",
    send_feedback: "Inviare feedback",
    to: "A {}",
    notes: "note {}/{}",
    in_appendix: "+ {} in appendice",
    nouns: [("schermata", "schermate"), ("board", "board"), ("pagina", "pagine"), ("stato", "stati"), ("domanda aperta", "domande aperte")],
    months: ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"],
    date: "{} {} {}",
};

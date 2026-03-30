/// glyph metrics for a built-in PDF font.
/// widths are indexed by `WinAnsi` (Windows-1252) character code,
/// measured in 1/1000ths of a text unit.
#[allow(dead_code)]
pub struct BuiltinFontMetrics {
    pub widths: [u16; 256],
    pub ascent: i16,
    pub descent: i16,
    pub cap_height: i16,
}

const HELVETICA_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     278,  278,  355,  556,  556,  889,  667,  191,  333,  333,  389,  584,  278,  333,  278,  278,
     556,  556,  556,  556,  556,  556,  556,  556,  556,  556,  278,  278,  584,  584,  584,  556,
    1015,  667,  667,  722,  722,  667,  611,  778,  722,  278,  500,  667,  556,  833,  722,  778,
     667,  778,  722,  667,  611,  722,  667,  944,  667,  667,  611,  278,  278,  278,  469,  556,
     333,  556,  556,  500,  556,  556,  278,  556,  556,  222,  222,  500,  222,  833,  556,  556,
     556,  556,  333,  500,  278,  556,  500,  722,  500,  500,  500,  334,  260,  334,  584,    0,
     556,    0,  222,  556,  333, 1000,  556,  556,  333, 1000,  667,  333, 1000,    0,  611,    0,
       0,  222,  222,  333,  333,  350,  556, 1000,  333, 1000,  500,  333,  944,    0,  500,  667,
     278,  333,  556,  556,  556,  556,  260,  556,  333,  737,  370,  556,  584,  333,  737,  333,
     400,  584,  333,  333,  333,  556,  537,  278,  333,  333,  365,  556,  834,  834,  834,  611,
     667,  667,  667,  667,  667,  667, 1000,  722,  667,  667,  667,  667,  278,  278,  278,  278,
     722,  722,  778,  778,  778,  778,  778,  584,  778,  722,  722,  722,  722,  667,  667,  611,
     556,  556,  556,  556,  556,  556,  889,  500,  556,  556,  556,  556,  278,  278,  278,  278,
     556,  556,  556,  556,  556,  556,  556,  584,  611,  556,  556,  556,  556,  500,  556,  500,
];

const HELVETICA_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: HELVETICA_WIDTHS,
    ascent: 718,
    descent: -207,
    cap_height: 718,
};

const HELVETICA_BOLD_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     278,  333,  474,  556,  556,  889,  722,  238,  333,  333,  389,  584,  278,  333,  278,  278,
     556,  556,  556,  556,  556,  556,  556,  556,  556,  556,  333,  333,  584,  584,  584,  611,
     975,  722,  722,  722,  722,  667,  611,  778,  722,  278,  556,  722,  611,  833,  722,  778,
     667,  778,  722,  667,  611,  722,  667,  944,  667,  667,  611,  333,  278,  333,  584,  556,
     333,  556,  611,  556,  611,  556,  333,  611,  611,  278,  278,  556,  278,  889,  611,  611,
     611,  611,  389,  556,  333,  611,  556,  778,  556,  556,  500,  389,  280,  389,  584,    0,
     556,    0,  278,  556,  500, 1000,  556,  556,  333, 1000,  667,  333, 1000,    0,  611,    0,
       0,  278,  278,  500,  500,  350,  556, 1000,  333, 1000,  556,  333,  944,    0,  500,  667,
     278,  333,  556,  556,  556,  556,  280,  556,  333,  737,  370,  556,  584,  333,  737,  333,
     400,  584,  333,  333,  333,  611,  556,  278,  333,  333,  365,  556,  834,  834,  834,  611,
     722,  722,  722,  722,  722,  722, 1000,  722,  667,  667,  667,  667,  278,  278,  278,  278,
     722,  722,  778,  778,  778,  778,  778,  584,  778,  722,  722,  722,  722,  667,  667,  611,
     556,  556,  556,  556,  556,  556,  889,  556,  556,  556,  556,  556,  278,  278,  278,  278,
     611,  611,  611,  611,  611,  611,  611,  584,  611,  611,  611,  611,  611,  556,  611,  556,
];

const HELVETICA_BOLD_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: HELVETICA_BOLD_WIDTHS,
    ascent: 718,
    descent: -207,
    cap_height: 718,
};

const TIMES_ROMAN_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     250,  333,  408,  500,  500,  833,  778,  180,  333,  333,  500,  564,  250,  333,  250,  278,
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  278,  278,  564,  564,  564,  444,
     921,  722,  667,  667,  722,  611,  556,  722,  722,  333,  389,  722,  611,  889,  722,  722,
     556,  722,  667,  556,  611,  722,  722,  944,  722,  722,  611,  333,  278,  333,  469,  500,
     333,  444,  500,  444,  500,  444,  333,  500,  500,  278,  278,  500,  278,  778,  500,  500,
     500,  500,  333,  389,  278,  500,  500,  722,  500,  500,  444,  480,  200,  480,  541,    0,
     500,    0,  333,  500,  444, 1000,  500,  500,  333, 1000,  556,  333,  889,    0,  611,    0,
       0,  333,  333,  444,  444,  350,  500, 1000,  333,  980,  389,  333,  722,    0,  444,  722,
     250,  333,  500,  500,  500,  500,  200,  500,  333,  760,  276,  500,  564,  333,  760,  333,
     400,  564,  300,  300,  333,  500,  453,  250,  333,  300,  310,  500,  750,  750,  750,  444,
     722,  722,  722,  722,  722,  722,  889,  667,  611,  611,  611,  611,  333,  333,  333,  333,
     722,  722,  722,  722,  722,  722,  722,  564,  722,  722,  722,  722,  722,  722,  556,  500,
     444,  444,  444,  444,  444,  444,  667,  444,  444,  444,  444,  444,  278,  278,  278,  278,
     500,  500,  500,  500,  500,  500,  500,  564,  500,  500,  500,  500,  500,  500,  500,  500,
];

const TIMES_ROMAN_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: TIMES_ROMAN_WIDTHS,
    ascent: 683,
    descent: -217,
    cap_height: 662,
};

const COURIER_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,    0,
     600,    0,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,    0,  600,    0,
       0,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,    0,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
     600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,  600,
];

const COURIER_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: COURIER_WIDTHS,
    ascent: 629,
    descent: -157,
    cap_height: 562,
};

const TIMES_BOLD_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     250,  333,  555,  500,  500, 1000,  833,  333,  333,  333,  500,  570,  250,  333,  250,  278,
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  333,  333,  570,  570,  570,  500,
     930,  722,  667,  722,  722,  667,  611,  778,  778,  389,  500,  778,  667,  944,  722,  778,
     611,  778,  722,  556,  667,  722,  722, 1000,  722,  722,  667,  333,  278,  333,  581,  500,
     333,  500,  556,  444,  556,  444,  333,  500,  556,  278,  333,  556,  278,  833,  556,  500,
     556,  556,  444,  389,  333,  556,  500,  722,  500,  500,  444,  394,  220,  394,  520,    0,
     500,    0,  333,  500,  500, 1000,  500,  500,  333, 1000,  556,  333, 1000,    0,  667,    0,
       0,  333,  333,  500,  500,  350,  500, 1000,  333, 1000,  389,  333,  722,    0,  444,  722,
     250,  333,  500,  500,  500,  500,  220,  500,  333,  747,  300,  500,  570,  333,  747,  333,
     400,  570,  300,  300,  333,  556,  540,  250,  333,  300,  330,  500,  750,  750,  750,  500,
     722,  722,  722,  722,  722,  722, 1000,  722,  667,  667,  667,  667,  389,  389,  389,  389,
     722,  722,  778,  778,  778,  778,  778,  570,  778,  722,  722,  722,  722,  722,  611,  556,
     500,  500,  500,  500,  500,  500,  722,  444,  444,  444,  444,  444,  278,  278,  278,  278,
     500,  556,  500,  500,  500,  500,  500,  570,  500,  556,  556,  556,  556,  500,  556,  500,
];

const TIMES_BOLD_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: TIMES_BOLD_WIDTHS,
    ascent: 683,
    descent: -217,
    cap_height: 676,
};

const TIMES_ITALIC_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     250,  333,  420,  500,  500,  833,  778,  333,  333,  333,  500,  675,  250,  333,  250,  278,
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  333,  333,  675,  675,  675,  500,
     920,  611,  611,  667,  722,  611,  611,  722,  722,  333,  444,  667,  556,  833,  667,  722,
     611,  722,  611,  500,  556,  722,  611,  833,  611,  556,  556,  389,  278,  389,  422,  500,
     333,  500,  500,  444,  500,  444,  278,  500,  500,  278,  278,  444,  278,  722,  500,  500,
     500,  500,  389,  389,  278,  500,  444,  667,  444,  444,  389,  400,  275,  400,  541,    0,
     500,    0,  333,  500,  556,  889,  500,  500,  333, 1000,  500,  333,  944,    0,  556,    0,
       0,  333,  333,  556,  556,  350,  500,  889,  333,  980,  389,  333,  667,    0,  389,  556,
     250,  389,  500,  500,  500,  500,  275,  500,  333,  760,  276,  500,  675,  333,  760,  333,
     400,  675,  300,  300,  333,  500,  523,  250,  333,  300,  310,  500,  750,  750,  750,  500,
     611,  611,  611,  611,  611,  611,  889,  667,  611,  611,  611,  611,  333,  333,  333,  333,
     722,  667,  722,  722,  722,  722,  722,  675,  722,  722,  722,  722,  722,  556,  611,  500,
     500,  500,  500,  500,  500,  500,  667,  444,  444,  444,  444,  444,  278,  278,  278,  278,
     500,  500,  500,  500,  500,  500,  500,  675,  500,  500,  500,  500,  500,  444,  500,  444,
];

const TIMES_ITALIC_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: TIMES_ITALIC_WIDTHS,
    ascent: 683,
    descent: -217,
    cap_height: 653,
};

const TIMES_BOLD_ITALIC_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     250,  389,  555,  500,  500,  833,  778,  333,  333,  333,  500,  570,  250,  333,  250,  278,
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  333,  333,  570,  570,  570,  500,
     832,  667,  667,  667,  722,  667,  667,  722,  778,  389,  500,  667,  611,  889,  722,  722,
     611,  722,  667,  556,  611,  722,  667,  889,  667,  611,  611,  333,  278,  333,  570,  500,
     333,  500,  500,  444,  500,  444,  333,  500,  556,  278,  278,  500,  278,  778,  556,  500,
     500,  500,  389,  389,  278,  556,  444,  667,  500,  444,  389,  348,  220,  348,  570,    0,
     500,    0,  333,  500,  500, 1000,  500,  500,  333, 1000,  556,  333,  944,    0,  611,    0,
       0,  333,  333,  500,  500,  350,  500, 1000,  333, 1000,  389,  333,  722,    0,  389,  611,
     250,  389,  500,  500,  500,  500,  220,  500,  333,  747,  266,  500,  606,  333,  747,  333,
     400,  570,  300,  300,  333,  576,  500,  250,  333,  300,  300,  500,  750,  750,  750,  500,
     667,  667,  667,  667,  667,  667,  944,  667,  667,  667,  667,  667,  389,  389,  389,  389,
     722,  722,  722,  722,  722,  722,  722,  570,  722,  722,  722,  722,  722,  611,  611,  500,
     500,  500,  500,  500,  500,  500,  722,  444,  444,  444,  444,  444,  278,  278,  278,  278,
     500,  556,  500,  500,  500,  500,  500,  570,  500,  556,  556,  556,  556,  444,  500,  444,
];

const TIMES_BOLD_ITALIC_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: TIMES_BOLD_ITALIC_WIDTHS,
    ascent: 683,
    descent: -217,
    cap_height: 669,
};

const SYMBOL_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     250,  333,  713,  500,  549,  833,  778,  439,  333,  333,  500,  549,  250,  549,  250,  278,
     500,  500,  500,  500,  500,  500,  500,  500,  500,  500,  278,  278,  549,  549,  549,  444,
     549,  722,  667,  722,  612,  611,  763,  603,  722,  333,  631,  722,  686,  889,  722,  722,
     768,  741,  556,  592,  611,  690,  439,  768,  645,  795,  611,  333,  863,  333,  658,  500,
     500,  631,  549,  549,  494,  439,  521,  411,  603,  329,  603,  549,  549,  576,  521,  549,
     549,  521,  549,  603,  439,  576,  713,  686,  493,  686,  494,  480,  200,  480,  549,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     750,  620,  247,  549,  167,  713,  500,  753,  753,  753,  753, 1042,  987,  603,  987,  603,
     400,  549,  411,  549,  549,  713,  494,  460,  549,  549,  549,  549, 1000,  603, 1000,  658,
     823,  686,  795,  987,  768,  768,  823,  768,  768,  713,  713,  713,  713,  713,  713,  713,
     768,  713,  790,  790,  890,  823,  549,  250,  713,  603,  603, 1042,  987,  603,  987,  603,
     494,  329,  790,  790,  786,  713,  384,  384,  384,  384,  384,  384,  494,  494,  494,  494,
       0,  329,  274,  686,  686,  686,  384,  384,  384,  384,  384,  384,  494,  494,  494,    0,
];

/// symbol uses its own encoding; ascent/descent estimated from font bbox.
const SYMBOL_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: SYMBOL_WIDTHS,
    ascent: 693,
    descent: -216,
    cap_height: 653,
};

const ZAPF_DINGBATS_WIDTHS: [u16; 256] = [
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
     278,  974,  961,  974,  980,  719,  789,  790,  791,  690,  960,  939,  549,  855,  911,  933,
     911,  945,  974,  755,  846,  762,  761,  571,  677,  763,  760,  759,  754,  494,  552,  537,
     577,  692,  786,  788,  788,  790,  793,  794,  816,  823,  789,  841,  823,  833,  816,  831,
     923,  744,  723,  749,  790,  792,  695,  776,  768,  792,  759,  707,  708,  682,  701,  826,
     815,  789,  789,  707,  687,  696,  689,  786,  787,  713,  791,  785,  791,  873,  761,  762,
     762,  759,  759,  892,  892,  788,  784,  438,  138,  277,  415,  392,  392,  668,  668,    0,
     390,  390,  317,  317,  276,  276,  509,  509,  410,  410,  234,  234,  334,  334,    0,    0,
       0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,    0,
       0,  732,  544,  544,  910,  667,  760,  760,  776,  595,  694,  626,  788,  788,  788,  788,
     788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,
     788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,  788,
     788,  788,  788,  788,  894,  838, 1016,  458,  748,  924,  748,  918,  927,  928,  928,  834,
     873,  828,  924,  924,  917,  930,  931,  463,  883,  836,  836,  867,  867,  696,  696,  874,
       0,  874,  760,  946,  771,  865,  771,  888,  967,  888,  831,  873,  927,  970,  918,    0,
];

/// zapfdingbats uses its own encoding; ascent/descent estimated from font bbox.
const ZAPF_DINGBATS_METRICS: BuiltinFontMetrics = BuiltinFontMetrics {
    widths: ZAPF_DINGBATS_WIDTHS,
    ascent: 820,
    descent: -143,
    cap_height: 705,
};

/// Resolve a built-in font variant given bold/italic flags.
#[allow(dead_code)]
///
/// Merges flags from the base font name: e.g. `("Helvetica-Bold", false, true)`
/// returns `"Helvetica-BoldOblique"` because the base is already bold.
pub fn resolve_builtin_variant(base_font: &str, bold: bool, italic: bool) -> Option<&'static str> {
    let (family, base_bold, base_italic) = if base_font.starts_with("Helvetica") {
        (
            "Helvetica",
            base_font.contains("Bold"),
            base_font.contains("Oblique"),
        )
    } else if base_font.starts_with("Times") {
        (
            "Times",
            base_font.contains("Bold"),
            base_font.contains("Italic"),
        )
    } else if base_font.starts_with("Courier") {
        (
            "Courier",
            base_font.contains("Bold"),
            base_font.contains("Oblique"),
        )
    } else if base_font == "Symbol" {
        return Some("Symbol");
    } else if base_font == "ZapfDingbats" {
        return Some("ZapfDingbats");
    } else {
        return None;
    };

    let eff_bold = bold || base_bold;
    let eff_italic = italic || base_italic;

    match (family, eff_bold, eff_italic) {
        ("Helvetica", false, false) => Some("Helvetica"),
        ("Helvetica", true, false) => Some("Helvetica-Bold"),
        ("Helvetica", false, true) => Some("Helvetica-Oblique"),
        ("Helvetica", true, true) => Some("Helvetica-BoldOblique"),
        ("Times", false, false) => Some("Times-Roman"),
        ("Times", true, false) => Some("Times-Bold"),
        ("Times", false, true) => Some("Times-Italic"),
        ("Times", true, true) => Some("Times-BoldItalic"),
        ("Courier", false, false) => Some("Courier"),
        ("Courier", true, false) => Some("Courier-Bold"),
        ("Courier", false, true) => Some("Courier-Oblique"),
        ("Courier", true, true) => Some("Courier-BoldOblique"),
        _ => None,
    }
}

/// measure a string's width in points using a built-in font's metrics.
pub fn measure_str(text: &str, font_name: &str, font_size: f32) -> Option<f32> {
    let metrics = get_builtin_metrics(font_name)?;
    let width: u32 = text.bytes().map(|b| u32::from(metrics.widths[b as usize])).sum();
    Some(width as f32 * font_size / 1000.0)
}

/// look up metrics for a built-in PDF font.
/// oblique/italic variants share widths with their upright counterpart.
/// all courier variants share the same monospace widths.
pub fn get_builtin_metrics(font_name: &str) -> Option<&'static BuiltinFontMetrics> {
    match font_name {
        "Helvetica" | "Helvetica-Oblique" => Some(&HELVETICA_METRICS),
        "Helvetica-Bold" | "Helvetica-BoldOblique" => Some(&HELVETICA_BOLD_METRICS),
        "Times-Roman" => Some(&TIMES_ROMAN_METRICS),
        "Times-Bold" => Some(&TIMES_BOLD_METRICS),
        "Times-Italic" => Some(&TIMES_ITALIC_METRICS),
        "Times-BoldItalic" => Some(&TIMES_BOLD_ITALIC_METRICS),
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => {
            Some(&COURIER_METRICS)
        }
        "Symbol" => Some(&SYMBOL_METRICS),
        "ZapfDingbats" => Some(&ZAPF_DINGBATS_METRICS),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_helvetica_bold() {
        assert_eq!(
            resolve_builtin_variant("Helvetica", true, false),
            Some("Helvetica-Bold")
        );
    }

    #[test]
    fn resolve_helvetica_italic() {
        assert_eq!(
            resolve_builtin_variant("Helvetica", false, true),
            Some("Helvetica-Oblique")
        );
    }

    #[test]
    fn resolve_helvetica_bold_italic() {
        assert_eq!(
            resolve_builtin_variant("Helvetica", true, true),
            Some("Helvetica-BoldOblique")
        );
    }

    #[test]
    fn resolve_helvetica_plain() {
        assert_eq!(
            resolve_builtin_variant("Helvetica", false, false),
            Some("Helvetica")
        );
    }

    #[test]
    fn resolve_times_variants() {
        assert_eq!(
            resolve_builtin_variant("Times-Roman", false, false),
            Some("Times-Roman")
        );
        assert_eq!(
            resolve_builtin_variant("Times-Roman", true, false),
            Some("Times-Bold")
        );
        assert_eq!(
            resolve_builtin_variant("Times-Roman", false, true),
            Some("Times-Italic")
        );
        assert_eq!(
            resolve_builtin_variant("Times-Roman", true, true),
            Some("Times-BoldItalic")
        );
    }

    #[test]
    fn resolve_courier_variants() {
        assert_eq!(
            resolve_builtin_variant("Courier", false, false),
            Some("Courier")
        );
        assert_eq!(
            resolve_builtin_variant("Courier", true, false),
            Some("Courier-Bold")
        );
        assert_eq!(
            resolve_builtin_variant("Courier", false, true),
            Some("Courier-Oblique")
        );
        assert_eq!(
            resolve_builtin_variant("Courier", true, true),
            Some("Courier-BoldOblique")
        );
    }

    #[test]
    fn resolve_already_bold_plus_italic() {
        assert_eq!(
            resolve_builtin_variant("Helvetica-Bold", false, true),
            Some("Helvetica-BoldOblique")
        );
    }

    #[test]
    fn resolve_already_italic_plus_bold() {
        assert_eq!(
            resolve_builtin_variant("Times-Italic", true, false),
            Some("Times-BoldItalic")
        );
    }

    #[test]
    fn resolve_symbol_unchanged() {
        assert_eq!(
            resolve_builtin_variant("Symbol", true, true),
            Some("Symbol")
        );
    }

    #[test]
    fn resolve_zapfdingbats_unchanged() {
        assert_eq!(
            resolve_builtin_variant("ZapfDingbats", true, false),
            Some("ZapfDingbats")
        );
    }

    #[test]
    fn resolve_unknown_font_returns_none() {
        assert_eq!(resolve_builtin_variant("FakeFont", false, false), None);
    }
}
